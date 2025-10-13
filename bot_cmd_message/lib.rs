use bot_core::color_parameter::HexColorParameter;
use bot_core::{CmdContext, UserData};
use eyre::Result;
use poise::serenity_prelude::all::Builder;
use poise::serenity_prelude::{CreateEmbed, CreateMessage, GuildChannel};

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
