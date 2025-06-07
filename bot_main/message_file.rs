use eyre::{OptionExt as _, Result};
use poise::serenity_prelude::{
    Builder, CacheHttp, ChannelId, CreateAttachment, EditMessage, FormattedTimestamp, GuildId,
    Message, MessageId,
};

/// File storage using a Discord message attachment
pub struct MessageFile {
    pub filename: String,
    pub guild_id: Option<GuildId>,
    pub channel_id: ChannelId,
    pub message_id: MessageId,
}

impl MessageFile {
    /// Create from an existing message with an attachment
    pub fn from_message(msg: &Message) -> Result<Self> {
        let attachment = msg.attachments.first().ok_or_eyre("No attachment found")?;
        Ok(Self {
            filename: attachment.filename.clone(),
            guild_id: msg.guild_id,
            channel_id: msg.channel_id,
            message_id: msg.id,
        })
    }

    /// Create a new message with an attachment
    pub async fn create(
        http: &impl CacheHttp,
        filename: String,
        guild_id: Option<GuildId>,
        channel_id: ChannelId,
        content: String,
    ) -> Result<MessageFile> {
        let message_id = channel_id.say(http, "📝").await?.id;
        let mut this = Self { filename, guild_id, channel_id, message_id };
        this.write(http, content).await?;
        Ok(this)
    }

    /// Read the message attachment
    pub async fn read(&self, http: &impl CacheHttp) -> Result<Vec<u8>> {
        let message = self.channel_id.message(http, self.message_id).await?;
        let attachment = message.attachments.first().ok_or_eyre("No attachment found")?;
        Ok(attachment.download().await?)
    }

    /// Update the message attachment
    pub async fn write(&mut self, http: &impl CacheHttp, data: impl Into<Vec<u8>>) -> Result<()> {
        EditMessage::new()
            .new_attachment(CreateAttachment::bytes(data, self.filename.clone()))
            .content(format!("📝 {}", FormattedTimestamp::now()))
            .execute(http, (self.channel_id, self.message_id, None))
            .await?;
        Ok(())
    }
}
