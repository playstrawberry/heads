//! Strawberry Heads (first-party): the player's head before the name in chat, drawn by vanilla
//! 26.3 clients with no mod.
//!
//! - **Chat**: always on, for everyone. On the chat render event (`AsyncPlayerChatEvent`, after
//!   every other plugin's format at `highest`), the player's name in the finished line is
//!   replaced by the head and the name (`TextComponent::replace_text`), so the name keeps its
//!   colour, link, hover and click; a line without the name gets the head in front.
//! - **Heads**: 26.3's object component (`TextComponent::player_head`) from the player's name, id
//!   and the skin the server holds (Mojang, the engine's offline skins, SkinsRestorer).
//! - **Cache**: the last skin seen for every player (`cache`), refreshed at join, at every chat
//!   and by `/heads refresh <player>`, kept in `data/heads.tsv`.
//! - **API**: other plugins ask with the plugin message `head <name or uuid>` (`api`).
//! - **Tab list**: nothing to do here: the engine sends every entry's skin and encrypts every
//!   connection, so the client draws the faces itself.
//!
//! Written from the feature list alone (no other heads plugin was read).

pub mod api;
pub mod cache;
pub mod lang;

use std::sync::{Arc, Mutex, PoisonError};

use strawberry_plugin_api::{
    Context, Player, Plugin, PluginMetadata, Result, Server,
    command::{Arg, ArgumentType, Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    commands::CommandHandler,
    events::{AsyncPlayerChatEvent, EventData, EventHandler, EventPriority, PlayerJoinEvent},
    permission::{Permission, PermissionDefault},
    register_plugin,
    text::{HeadProfile, TextComponent},
};

use cache::{Head, HeadCache};

const CACHE_FILE: &str = "data/heads.tsv";
const ADMIN_NODE: &str = "strawberry-heads:admin";

type Shared = Arc<Mutex<HeadCache>>;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn load() -> HeadCache {
    std::fs::read_to_string(CACHE_FILE)
        .map_or_else(|_| HeadCache::default(), |t| HeadCache::from_text(&t))
}

fn save(cache: &HeadCache) {
    let tmp = "data/heads.tsv.tmp";
    if std::fs::write(tmp, cache.to_text()).is_ok() {
        let _ = std::fs::rename(tmp, CACHE_FILE);
    }
}

/// The player as the cache keeps it, with the skin the server holds now.
fn head_of(player: &Player) -> Head {
    let skin = player.get_skin();
    Head {
        id: strawberry_plugin_api::uuid::to_string(player.get_id()),
        name: player.get_name(),
        seen: now(),
        textures: skin.as_ref().map(|s| s.value.clone()),
        signature: skin.and_then(|s| s.signature),
    }
}

/// Puts the player into the cache (and the file, when a name or skin changed).
fn see(shared: &Shared, player: &Player) -> Head {
    let head = head_of(player);
    let mut cache = shared.lock().unwrap_or_else(PoisonError::into_inner);
    if cache.see(head.clone()) {
        save(&cache);
    }
    head
}

/// The head component: with the skin when the profile takes it, else by name and id only (the
/// client then finds the skin or draws the default face). White, so a coloured name around it
/// does not tint the face.
fn head_component(head: &Head) -> Option<TextComponent> {
    let profile = |textures: bool| HeadProfile {
        name: Some(head.name.clone()),
        id: Some(head.id.clone()),
        textures: textures.then(|| head.textures.clone()).flatten(),
        textures_signature: textures.then(|| head.signature.clone()).flatten(),
    };
    let component = TextComponent::player_head(&profile(true), true)
        .or_else(|_| TextComponent::player_head(&profile(false), true))
        .ok()?;
    Some(component.color_named(strawberry_plugin_api::common::NamedColor::White))
}

struct ChatHeads(Shared);

impl EventHandler<AsyncPlayerChatEvent> for ChatHeads {
    fn handle(
        &self,
        _server: Server,
        mut event: EventData<AsyncPlayerChatEvent>,
    ) -> EventData<AsyncPlayerChatEvent> {
        if event.cancelled {
            return event;
        }
        let head = see(&self.0, &event.player);
        let Some(icon) = head_component(&head) else {
            return event;
        };
        let line_text = event.format.get_text();
        let display = event.player.get_display_name().get_text();
        let name = [display.as_str(), head.name.as_str()]
            .into_iter()
            .find(|n| !n.is_empty() && line_text.contains(n));
        if let Some(name) = name {
            let replacement = TextComponent::text("")
                .add_child(icon)
                .add_text(&format!(" {name}"));
            event.format.replace_text(name, &replacement, true);
            event
        } else {
            let old = std::mem::replace(&mut event.format, TextComponent::text(""));
            event.format = TextComponent::text("")
                .add_child(icon)
                .add_text(" ")
                .add_child(old);
            event
        }
    }
}

struct Seen(Shared);

impl EventHandler<PlayerJoinEvent> for Seen {
    fn handle(
        &self,
        _server: Server,
        event: EventData<PlayerJoinEvent>,
    ) -> EventData<PlayerJoinEvent> {
        see(&self.0, &event.player);
        event
    }
}

fn say(sender: &CommandSender, key: &str, args: &[&str]) {
    let locale = format!("{:?}", sender.get_locale());
    let text = lang::text(lang::language(&locale), key, args);
    sender.send_message(TextComponent::text(&text));
}

struct Status(Shared);

impl CommandHandler for Status {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let count = self.0.lock().unwrap_or_else(PoisonError::into_inner).len();
        say(&sender, "status", &[&count.to_string()]);
        say(&sender, "usage", &[]);
        Ok(1)
    }
}

struct Refresh(Shared);

impl CommandHandler for Refresh {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let Arg::Players(players) = args.get_value("player") else {
            say(&sender, "usage", &[]);
            return Ok(0);
        };
        if players.is_empty() {
            say(&sender, "not_online", &["?"]);
            return Ok(0);
        }
        for player in &players {
            let head = see(&self.0, player);
            say(&sender, "refreshed", &[&head.name]);
        }
        Ok(i32::try_from(players.len()).unwrap_or(i32::MAX))
    }
}

struct StrawberryHeads {
    cache: Shared,
}

impl Plugin for StrawberryHeads {
    fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HeadCache::default())),
        }
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "strawberry-heads".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["Strawberry Interactive LLC".into()],
            description: "The player's head before the name in chat.".into(),
            dependencies: Vec::new(),
            permissions: Vec::new(),
        }
    }

    fn on_load(&self, context: Context) -> Result<()> {
        *self.cache.lock().unwrap_or_else(PoisonError::into_inner) = load();
        context.register_permission(&Permission {
            node: ADMIN_NODE.into(),
            description: "Use /heads and /heads refresh".into(),
            default: PermissionDefault::Op(strawberry_plugin_api::permission::PermissionLevel::Two),
            children: vec![],
        })?;
        let command = Command::new(&["heads".to_string()], "Strawberry Heads: the head cache.")
            .execute(Status(Arc::clone(&self.cache)))
            .then(
                CommandNode::literal("refresh").then(
                    CommandNode::argument("player", &ArgumentType::Players)
                        .execute(Refresh(Arc::clone(&self.cache))),
                ),
            );
        context.register_command(command, ADMIN_NODE);
        context.register_event_handler::<AsyncPlayerChatEvent, _>(
            ChatHeads(Arc::clone(&self.cache)),
            EventPriority::Highest,
            true,
        )?;
        context.register_event_handler::<PlayerJoinEvent, _>(
            Seen(Arc::clone(&self.cache)),
            EventPriority::Normal,
            false,
        )?;
        Ok(())
    }

    fn handle_ipc_message(
        &self,
        _from: strawberry_plugin_api::wit::PluginId,
        payload: strawberry_plugin_api::wit::IpcMessage,
    ) -> std::result::Result<strawberry_plugin_api::wit::IpcMessage, String> {
        let cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        api::handle(&cache, &payload)
    }
}

register_plugin!(StrawberryHeads);
