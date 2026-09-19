use eyre::Result;
use poise::CreateReply;
use poise::serenity_prelude::prelude::CacheHttp;
use poise::serenity_prelude::{
    Builder as _, ChannelId, CommandInteraction, ComponentInteraction, CreateActionRow, CreateAllowedMentions,
    CreateAttachment, CreateEmbed, CreateInteractionResponse, CreateMessage, CreatePoll, Message, ModalInteraction,
    create_poll,
};

/// Convenient builder that's similar to [`poise::CreateReply`] but modified.
#[derive(Default, Clone)]
pub struct Msg {
    content: Option<String>,
    embeds: Vec<CreateEmbed>,
    attachments: Vec<CreateAttachment>,
    ephemeral: Option<bool>,
    components: Option<Vec<CreateActionRow>>,
    allowed_mentions: Option<CreateAllowedMentions>,
    poll: Option<CreatePoll<create_poll::Ready>>,
}

impl Msg {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the content of the message.
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// Adds an embed to the message.
    ///
    /// Existing embeds are kept.
    pub fn embed(mut self, embed: CreateEmbed) -> Self {
        self.embeds.push(embed);
        self
    }

    pub fn embeds(mut self, embeds: impl IntoIterator<Item = CreateEmbed>) -> Self {
        self.embeds.extend(embeds);
        self
    }

    /// Set components (buttons and select menus) for this message.
    ///
    /// Any previously set components will be overwritten.
    pub fn components(mut self, components: Vec<CreateActionRow>) -> Self {
        self.components = Some(components);
        self
    }

    /// Add an attachment.
    pub fn attachment(mut self, attachment: CreateAttachment) -> Self {
        self.attachments.push(attachment);
        self
    }

    /// Add multiple attachments.
    pub fn attachments(mut self, attachments: impl IntoIterator<Item = CreateAttachment>) -> Self {
        self.attachments.extend(attachments);
        self
    }

    /// Toggles whether the message is an ephemeral response (only invoking user can see it).
    ///
    /// This only has an effect in slash commands!
    pub fn ephemeral(mut self, ephemeral: bool) -> Self {
        self.ephemeral = Some(ephemeral);
        self
    }

    /// Set the allowed mentions for the message.
    ///
    /// See [`CreateAllowedMentions`] for more information.
    pub fn allowed_mentions(mut self, allowed_mentions: CreateAllowedMentions) -> Self {
        self.allowed_mentions = Some(allowed_mentions);
        self
    }

    /// Adds a poll to the message. Only one poll can be added per message.
    ///
    /// See [`CreatePoll`] for more information on creating and configuring a poll.
    pub fn poll(mut self, poll: CreatePoll<create_poll::Ready>) -> Self {
        self.poll = Some(poll);
        self
    }

    pub fn to_reply(self) -> CreateReply {
        let mut reply = CreateReply::new();
        if let Some(content) = self.content {
            reply = reply.content(content);
        }
        if let Some(ephemeral) = self.ephemeral {
            reply = reply.ephemeral(ephemeral);
        }
        if let Some(components) = self.components {
            reply = reply.components(components);
        }
        if let Some(allowed_mentions) = self.allowed_mentions {
            reply = reply.allowed_mentions(allowed_mentions);
        }
        if let Some(poll) = self.poll {
            reply = reply.poll(poll);
        }
        for embed in self.embeds {
            reply = reply.embed(embed);
        }
        for attachment in self.attachments {
            reply = reply.attachment(attachment);
        }
        reply
    }

    pub async fn respond_to_command(self, ctx: impl CacheHttp, interaction: &CommandInteraction) -> Result<()> {
        Ok(CreateInteractionResponse::Message(self.to_reply().to_slash_initial_response(Default::default()))
            .execute(ctx, (interaction.id, &interaction.token))
            .await?)
    }

    pub async fn respond_to_component(self, ctx: impl CacheHttp, interaction: &ComponentInteraction) -> Result<()> {
        Ok(CreateInteractionResponse::Message(self.to_reply().to_slash_initial_response(Default::default()))
            .execute(ctx, (interaction.id, &interaction.token))
            .await?)
    }

    pub async fn followup_to_component(
        self,
        ctx: impl CacheHttp,
        interaction: &ComponentInteraction,
    ) -> Result<Message> {
        Ok(self
            .to_reply()
            .to_slash_followup_response(Default::default())
            .execute(ctx, (None, &interaction.token))
            .await?)
    }

    pub async fn followup_to_command(self, ctx: impl CacheHttp, interaction: &CommandInteraction) -> Result<Message> {
        Ok(self
            .to_reply()
            .to_slash_followup_response(Default::default())
            .execute(ctx, (None, &interaction.token))
            .await?)
    }

    pub async fn followup_to_modal(self, ctx: impl CacheHttp, interaction: &ModalInteraction) -> Result<Message> {
        Ok(self
            .to_reply()
            .to_slash_followup_response(Default::default())
            .execute(ctx, (None, &interaction.token))
            .await?)
    }

    pub async fn update_to_component(self, ctx: impl CacheHttp, interaction: &ComponentInteraction) -> Result<()> {
        Ok(CreateInteractionResponse::UpdateMessage(self.to_reply().to_slash_initial_response(Default::default()))
            .execute(ctx, (interaction.id, &interaction.token))
            .await?)
    }

    pub async fn edit_initial_modal_response(
        self,
        ctx: impl CacheHttp,
        interaction: &ModalInteraction,
    ) -> Result<Message> {
        Ok(self.to_reply().to_slash_initial_response_edit(Default::default()).execute(ctx, &interaction.token).await?)
    }

    pub async fn edit_initial_component_response(
        self,
        ctx: impl CacheHttp,
        interaction: &ComponentInteraction,
    ) -> Result<Message> {
        Ok(self.to_reply().to_slash_initial_response_edit(Default::default()).execute(ctx, &interaction.token).await?)
    }

    /// Note: the returned [`Message`] won't have its guild_id set correctly I think.
    pub async fn create_message(self, ctx: impl CacheHttp, channel_id: ChannelId) -> Result<Message> {
        let Msg {
            content,
            embeds,
            attachments,
            components,
            ephemeral: _, // not supported in prefix
            allowed_mentions,
            poll,
        } = self;

        let mut builder = CreateMessage::new();
        if let Some(content) = content {
            builder = builder.content(content);
        }
        if let Some(allowed_mentions) = allowed_mentions {
            builder = builder.allowed_mentions(allowed_mentions);
        }
        if let Some(components) = components {
            builder = builder.components(components);
        }
        if let Some(poll) = poll {
            builder = builder.poll(poll);
        }
        for attachment in attachments {
            builder = builder.add_file(attachment);
        }
        let create_message = builder.embeds(embeds);

        Ok(create_message.execute(ctx, (channel_id, None)).await?)
    }

    pub async fn edit_message(self, ctx: impl CacheHttp, message: &Message) -> Result<Message> {
        Ok(self
            .to_reply()
            .to_prefix_edit(Default::default())
            .execute(ctx, (message.channel_id, message.id, Some(message.author.id)))
            .await?)
    }
}
