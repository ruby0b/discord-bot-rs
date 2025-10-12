use crate::message_file::MessageFile;
use crate::util::{code_block_or_file, diff};
use bot_core::{CmdContext, OptionExt as _, State};
use eyre::{OptionExt as _, Result, WrapErr as _, ensure};
use poise::serenity_prelude::{
    Builder, Cache, CacheHttp, ChannelId, CreateAttachment, CreateAutocompleteResponse,
    CreateInputText, CreateInteractionResponse, CreateInteractionResponseFollowup,
    CreateQuickModal, GuildId, Http, InputTextStyle, InteractionId, Message, ModalInteraction,
};
use poise::{ChoiceParameter, CreateReply, serenity_prelude as serenity};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OnceCell, RwLock};

pub trait ConfigDataT = serde::Serialize
    + for<'a> serde::Deserialize<'a>
    + Default
    + Debug
    + Clone
    + PartialEq
    + Send
    + Sync
    + 'static;

pub struct GuildConfig<DataT: ConfigDataT>(RwLock<OnceCell<ConfigInner<DataT>>>);

#[derive(Debug)]
struct ConfigInner<DataT: ConfigDataT> {
    file: MessageFile,
    cache: DataT,
    dirty: bool,
}

impl<DataT: ConfigDataT> Default for GuildConfig<DataT> {
    fn default() -> Self {
        Self(RwLock::new(OnceCell::new()))
    }
}

impl<DataT: ConfigDataT> GuildConfig<DataT> {
    pub async fn init(
        &self,
        chttp: (&Arc<Cache>, &Http),
        guild_id: Option<GuildId>,
        channel_id: ChannelId,
    ) -> Result<()> {
        let pins = channel_id.pins(chttp.1).await?;
        let pinned_message = pins.into_iter().find_map(|m| {
            (m.author.id == chttp.0.current_user().id && !m.attachments.is_empty()).then_some(m)
        });

        match pinned_message {
            Some(m) => self.init_from_message(&chttp, &m).await,
            None => self.init_new_message(&chttp, guild_id, channel_id).await,
        }
    }

    pub async fn init_new_message(
        &self,
        chttp: &impl CacheHttp,
        guild_id: Option<GuildId>,
        channel_id: ChannelId,
    ) -> Result<()> {
        let config = self.0.write().await;

        let cache: DataT = Default::default();
        let file = MessageFile::create(
            chttp,
            format!("{CONFIG_NAME}.{CONFIG_EXT}"),
            guild_id,
            channel_id,
            to_yaml_string(&cache)?,
        )
        .await?;

        file.channel_id.pin(chttp.http(), file.message_id).await?;

        config.set(ConfigInner { file, cache, dirty: false })?;
        Ok(())
    }

    pub async fn init_from_message(&self, chttp: &impl CacheHttp, message: &Message) -> Result<()> {
        let config = self.0.write().await;
        let mut file = MessageFile::from_message(message)?;
        file.filename = format!("{CONFIG_NAME}.{CONFIG_EXT}");

        let bytes = file.read(chttp).await?;
        let old_str = String::from_utf8_lossy(&bytes);
        let cache: DataT =
            from_yaml_str(&old_str).inspect_err(|err| tracing::error!(?err)).unwrap_or_default();

        let new_str = to_yaml_string(&cache)?;
        if old_str != new_str {
            let link = file.message_id.link(file.channel_id, file.guild_id);
            let (content, files) = code_block_or_file(
                format!("✏️ Overwrote config: {link}"),
                diff(old_str.as_ref(), &new_str).as_bytes(),
                CONFIG_NAME,
                "diff",
            );
            let msg = serenity::CreateMessage::new().content(content).files(files).add_file(
                CreateAttachment::bytes(old_str.as_bytes(), format!("old_{}", file.filename)),
            );
            if let Err(why) = message.channel_id.send_message(chttp, msg).await {
                tracing::error!(%why, %old_str, "Failed to send old config");
            }
        }

        file.write(chttp, new_str).await?;

        config.set(ConfigInner { file, cache, dirty: false })?;
        Ok(())
    }

    pub async fn with<T>(&self, f: impl FnOnce(&DataT) -> Result<T>) -> Result<T> {
        let config = self.0.read().await;
        let config = config.get().ok_or_eyre("Uninitialized config")?;

        f(&config.cache)
    }

    pub async fn with_mut<T>(&self, f: impl FnOnce(&mut DataT) -> Result<T>) -> Result<T> {
        let mut config = self.0.write().await;
        let config = config.get_mut().ok_or_eyre("Uninitialized config")?;

        config.dirty = true;
        f(&mut config.cache)
    }

    pub async fn write_periodically(self: Arc<Self>, ctx: serenity::Context) {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            if !self.is_initialized().await {
                continue;
            }
            match self.write_if_dirty(&ctx).await {
                Ok(true) => tracing::debug!("Wrote config"),
                Ok(false) => {}
                Err(err) => tracing::error!("Failed to write config: {err:?}"),
            }
        }
    }

    async fn is_initialized(&self) -> bool {
        self.0.read().await.initialized()
    }

    async fn write_if_dirty(&self, http: &impl CacheHttp) -> Result<bool> {
        let mut config = self.0.write().await;
        let config = config.get_mut().ok_or_eyre("Uninitialized config")?;

        if config.dirty {
            config.file.write(http, to_yaml_string(&config.cache)?).await?;
            config.dirty = false;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

const CONFIG_NAME: &str = "config";
const CONFIG_EXT: &str = "yaml";

fn to_yaml_value<T: Serialize>(x: T) -> Result<serde_yml::Value> {
    Ok(serde_yml::to_value(x)?)
}

fn to_yaml_string<T: Serialize>(x: &T) -> Result<String> {
    Ok(serde_yml::to_string(x)?)
}

fn from_yaml_str<T: for<'a> Deserialize<'a>>(s: &str) -> Result<T> {
    Ok(serde_yml::from_str(s)?)
}

#[derive(Clone, Copy, Debug, ChoiceParameter)]
enum EditOperation {
    Show,
    Edit,
    Append,
    Insert,
}

/// Edit a config value in a modal dialog
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_GUILD",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn config<D: State<GuildConfig<impl ConfigDataT>>>(
    ctx: CmdContext<'_, D>,
    #[description = "Dot-separated config path"]
    #[autocomplete = autocomplete_config]
    path: String,
    #[description = "Operation to perform (default is Edit)"] //
    operation: Option<EditOperation>,
) -> Result<()> {
    let poise::Context::Application(app) = ctx else { return Ok(()) };

    let root = ctx.data().state().with(|cfg| to_yaml_value(cfg)).await?;
    let root_str = to_yaml_string(&root)?;
    let value = autocomplete_yaml::get_path(&root, path.split('.')).ok_or_eyre("Invalid path")?;
    let value_str = to_yaml_string(&value)?;

    let Some(int) = match operation {
        Some(EditOperation::Show) => {
            let (content, files) = code_block_or_file(
                format!("Value of `{path}`:"),
                value_str,
                CONFIG_NAME,
                CONFIG_EXT,
            );
            // I hate this, why are the builder interfaces so inconsistent :(
            let mut builder = CreateReply::new().content(content);
            for f in files {
                builder = builder.attachment(f);
            }
            ctx.send(builder).await?;
            return Ok(());
        }
        Some(EditOperation::Append) => {
            ensure!(value.is_sequence(), "Can only append to arrays");
            edit_in_modal(
                &ctx,
                app.interaction.id,
                &app.interaction.token,
                CreateQuickModal::new("Append").paragraph_field("Value"),
                |root, inputs| {
                    let value = from_yaml_str(inputs.get(0).some()?).wrap_err("Invalid value")?;
                    autocomplete_yaml::append_path(root, path.split("."), value)
                        .ok_or_eyre("Path has become invalid")
                },
            )
            .await
        }
        Some(EditOperation::Insert) => {
            ensure!(value.is_mapping(), "Can only insert into maps");
            edit_in_modal(
                &ctx,
                app.interaction.id,
                &app.interaction.token,
                CreateQuickModal::new("Insert").short_field("Key").paragraph_field("Value"),
                |root, inputs| {
                    let key = inputs.get(0).some()?;
                    let value = from_yaml_str(inputs.get(1).some()?).wrap_err("Invalid value")?;
                    autocomplete_yaml::insert_path(root, path.split("."), key, value)
                        .ok_or_eyre("Path has become invalid")
                },
            )
            .await
        }
        Some(EditOperation::Edit) | None => {
            edit_in_modal(
                &ctx,
                app.interaction.id,
                &app.interaction.token,
                CreateQuickModal::new("Edit").field(
                    CreateInputText::new(InputTextStyle::Paragraph, "Value", "").value(value_str),
                ),
                |root, inputs| {
                    let value = from_yaml_str(inputs.get(0).some()?).wrap_err("Invalid value")?;
                    autocomplete_yaml::set_path(root, path.split("."), value)
                        .ok_or_eyre("Path has become invalid")?;
                    Ok(())
                },
            )
            .await
        }
    }?
    else {
        return Ok(());
    };

    let new_root = ctx.data().state().with(|cfg| to_yaml_value(cfg)).await?;
    let new_root_str = to_yaml_string(&new_root)?;
    let diff = diff(&root_str, &new_root_str);
    let (content, files) =
        code_block_or_file(format!("✏️ Wrote `{path}`:"), diff, CONFIG_NAME, "diff");

    CreateInteractionResponseFollowup::new()
        .content(content)
        .files(files)
        .execute(ctx, (None, &int.token))
        .await?;

    Ok(())
}

async fn edit_in_modal<D: State<GuildConfig<impl ConfigDataT>>>(
    ctx: &CmdContext<'_, D>,
    inter_id: InteractionId,
    inter_token: &str,
    modal: CreateQuickModal,
    edit: impl FnOnce(&mut serde_yml::Value, Vec<String>) -> Result<()>,
) -> Result<Option<ModalInteraction>> {
    let Some(modal_response) = modal
        .timeout(Duration::from_secs(2 * 60))
        .execute(ctx.serenity_context(), inter_id, inter_token)
        .await?
    else {
        return Ok(None);
    };

    ctx.data()
        .state()
        .with_mut(|cfg| {
            let mut config_value = to_yaml_value(&cfg)?;
            edit(&mut config_value, modal_response.inputs)?;
            let new = Deserialize::deserialize(config_value)?;
            ensure!(cfg != &new, "No changes made");
            *cfg = new;
            Ok(())
        })
        .await?;

    modal_response.interaction.create_response(ctx, CreateInteractionResponse::Acknowledge).await?;

    Ok(Some(modal_response.interaction))
}

/// Restore a config backup
#[poise::command(
    prefix_command,
    required_permissions = "MANAGE_GUILD",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn restore<D: State<GuildConfig<impl ConfigDataT>>>(
    ctx: CmdContext<'_, D>,
) -> Result<()> {
    let poise::Context::Prefix(pre) = &ctx else { return Ok(()) };

    let attachment = pre
        .msg
        .attachments
        .first()
        .ok_or_eyre("You have to upload the backup alongside the command")?;

    let bytes = attachment.download().await?;
    let new = from_yaml_str(&String::from_utf8_lossy(&bytes))?;
    let new_str = to_yaml_string(&new)?;

    let old = pre
        .data()
        .state()
        .with_mut(|cfg| {
            ensure!(cfg != &new, "No differences found");
            let old = to_yaml_value(&cfg)?;
            *cfg = new;
            Ok(old)
        })
        .await?;
    let old_str = to_yaml_string(&old)?;

    let diff = diff(&old_str, &new_str);
    let (content, files) =
        code_block_or_file("✏️ Restored:".to_string(), diff, CONFIG_NAME, "diff");

    let mut reply = CreateReply::new().content(content);
    for f in files {
        reply = reply.attachment(f);
    }
    reply = reply.attachment(CreateAttachment::bytes(
        old_str.as_bytes(),
        format!("old_{CONFIG_NAME}.{CONFIG_EXT}"),
    ));
    ctx.send(reply).await?;

    Ok(())
}

async fn autocomplete_config<U: State<GuildConfig<impl ConfigDataT>>, E>(
    ctx: poise::Context<'_, U, E>,
    input: &str,
) -> CreateAutocompleteResponse {
    ctx.data()
        .state()
        .with(|cfg| {
            let root = to_yaml_value(cfg)?;
            let choices =
                autocomplete_yaml::autocomplete_value(&root, input).into_iter().take(25).collect();
            Ok(CreateAutocompleteResponse::new().set_choices(choices))
        })
        .await
        .inspect_err(|e| tracing::error!("Failed to auto-complete config: {e:?}"))
        .unwrap_or_default()
}

mod autocomplete_yaml {
    use itertools::Itertools as _;
    use poise::serenity_prelude::all::AutocompleteChoice;
    use std::iter::once;

    pub fn autocomplete_value(root: &serde_yml::Value, input: &str) -> Vec<AutocompleteChoice> {
        let path: Vec<&str> = input.split(".").collect();
        let (last, rest) = path.split_last().unwrap_or((&"", &[]));
        let value = get_path(root, rest.iter().copied());

        let keys: Vec<String> = value.map_or(vec![], |v| match v {
            serde_yml::Value::Mapping(obj) => {
                obj.iter().filter_map(|(k, _v)| k.as_str().map(ToString::to_string)).collect()
            }
            serde_yml::Value::Sequence(arr) => {
                arr.iter().enumerate().map(|(i, _v)| i.to_string()).collect()
            }
            _ => vec![],
        });

        let exactly_matches_a_key = keys.iter().any(|value| value == last);

        let mut choices = keys
            .into_iter()
            .filter(|value| value.starts_with(last))
            .map(|value| rest.iter().chain(once(&value.as_str())).join("."))
            .map(|value| AutocompleteChoice::new(value.clone(), value))
            .collect();

        if exactly_matches_a_key {
            let mut nested_choices = autocomplete_value(root, &(input.to_string() + "."));
            nested_choices.append(&mut choices);
            nested_choices
        } else {
            choices
        }
    }

    pub fn get_path(
        mut root: &serde_yml::Value,
        path: impl Iterator<Item = &str>,
    ) -> Option<&serde_yml::Value> {
        for key in path {
            let key = key.trim();
            if key.is_empty() {
                continue;
            }
            match root {
                serde_yml::Value::Mapping(t) => {
                    root = t.get(key)?;
                }
                serde_yml::Value::Sequence(arr) => {
                    if let Ok(index) = key.parse::<usize>() {
                        root = arr.get(index)?;
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        }

        Some(root)
    }

    pub fn get_path_mut(
        mut root: &mut serde_yml::Value,
        path: impl Iterator<Item = &str>,
    ) -> Option<&mut serde_yml::Value> {
        for key in path {
            let key = key.trim();
            if key.is_empty() {
                continue;
            }
            match root {
                serde_yml::Value::Mapping(t) => {
                    root = t.get_mut(key)?;
                }
                serde_yml::Value::Sequence(arr) => {
                    if let Ok(index) = key.parse::<usize>() {
                        root = arr.get_mut(index)?;
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        }

        Some(root)
    }

    #[must_use]
    pub fn set_path(
        root: &mut serde_yml::Value,
        path: impl Iterator<Item = &str>,
        new_value: serde_yml::Value,
    ) -> Option<serde_yml::Value> {
        Some(std::mem::replace(get_path_mut(root, path)?, new_value))
    }

    #[must_use]
    pub fn append_path(
        root: &mut serde_yml::Value,
        path: impl Iterator<Item = &str>,
        new_value: serde_yml::Value,
    ) -> Option<()> {
        match get_path_mut(root, path)? {
            serde_yml::Value::Sequence(arr) => {
                arr.push(new_value);
                Some(())
            }
            _ => None,
        }
    }

    #[must_use]
    pub fn insert_path(
        root: &mut serde_yml::Value,
        path: impl Iterator<Item = &str>,
        new_key: &str,
        new_value: serde_yml::Value,
    ) -> Option<()> {
        match get_path_mut(root, path)? {
            serde_yml::Value::Mapping(obj) => {
                obj.insert(serde_yml::Value::String(new_key.to_string()), new_value);
                Some(())
            }
            _ => None,
        }
    }
}
