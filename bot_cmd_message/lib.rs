use bot_core::choice_parameters::ButtonStyleParameter;
use bot_core::color_parameter::HexColorParameter;
use bot_core::ext::create_reply::CreateReplyExt;
use bot_core::{CmdContext, UserData};
use eyre::{Result, bail, ensure};
use poise::CreateReply;
use poise::serenity_prelude::{
    ActionRow, ActionRowComponent, Builder, ButtonStyle, CreateActionRow, CreateButton, CreateEmbed, CreateMessage,
    GuildChannel, Message, ReactionType,
};

/// Post a bot message
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_GUILD",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn message<D: UserData>(
    ctx: CmdContext<'_, D>,
    #[description = "Channel to send this message to"] channel: GuildChannel,
    #[description = "Message Content"] content: Option<String>,
    #[description = "Embed title"] embed_title: Option<String>,
    #[description = "Embed description"] embed_description: Option<String>,
    #[description = "Embed color (hex code)"] embed_color: Option<HexColorParameter>,
    #[description = "Embed thumbnail"] embed_thumbnail: Option<String>,
    #[description = "Embed image"] embed_image: Option<String>,
) -> Result<()> {
    let mut builder = CreateMessage::new();
    if let Some(c) = content {
        builder = builder.content(c);
    }
    let builder = builder.embed({
        let mut embed = CreateEmbed::new();
        if let Some(t) = embed_title {
            embed = embed.title(t);
        }
        if let Some(d) = embed_description {
            embed = embed.description(d);
        }
        if let Some(c) = embed_color {
            embed = embed.color(c);
        }
        if let Some(t) = embed_thumbnail {
            embed = embed.thumbnail(t);
        }
        if let Some(i) = embed_image {
            embed = embed.image(i);
        }
        embed
    });

    let msg = builder.execute(ctx, (channel.id, Some(channel.guild_id))).await?;

    ctx.say(format!("Message sent: {}", msg.link())).await?;
    Ok(())
}

/// Add a button to a bot message
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_GUILD",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn button<D: UserData>(
    ctx: CmdContext<'_, D>,
    #[description = "Link to a message sent by this bot"] bot_message: Message,
    #[description = "Button ID"] button_id: String,
    #[description = "Button label"] button_label: Option<String>,
    #[string]
    #[description = "Button emoji"]
    button_emoji: Option<ReactionType>,
    #[description = "Button style"] button_style: Option<ButtonStyleParameter>,
    #[description = "Clear all existing components of the message"] clear: Option<bool>,
) -> Result<()> {
    ctx.defer().await?;

    ensure!(bot_message.author.id == ctx.framework().bot_id(), "That message wasn't sent by me");

    let button = CreateButton::new(button_id).style(button_style.map_or(ButtonStyle::Primary, |s| s.into()));
    let button = match (button_label, button_emoji) {
        (Some(label), Some(emoji)) => button.label(label).emoji(emoji),
        (None, Some(emoji)) => button.emoji(emoji),
        (Some(label), None) => button.label(label),
        (None, None) => bail!("Buttons need at least a label or an emoji"),
    };

    let mut buttons = match (clear.unwrap_or(false), &bot_message.components[..]) {
        (true, _) | (false, []) => vec![],
        (false, [row]) => button_row_to_create_buttons_vec(row)?,
        (false, [_, _, ..]) => bail!("Message has more than 1 component row"),
    };
    buttons.push(button);

    CreateReply::new()
        .content(bot_message.content.clone())
        .embeds(bot_message.embeds.iter().cloned().map(|e| e.into()))
        .components(vec![CreateActionRow::Buttons(buttons)])
        .edit_message(ctx.serenity_context(), &bot_message)
        .await?;

    ctx.say(format!("Button added: {}", bot_message.link())).await?;
    Ok(())
}

fn button_row_to_create_buttons_vec(row: &ActionRow) -> Result<Vec<CreateButton>> {
    let mut buttons = vec![];
    for component in &row.components {
        match component {
            ActionRowComponent::Button(button) => {
                buttons.push(button.clone().into());
            }
            _ => bail!("Unexpected component: {component:?}"),
        }
    }
    Ok(buttons)
}
