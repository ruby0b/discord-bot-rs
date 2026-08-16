use eyre::Result;
use poise::CreateReply;
use poise::serenity_prelude::{
    Builder as _, CommandInteraction, ComponentInteraction, Context, CreateAttachment, CreateEmbed,
    CreateInteractionResponse, Message, ModalInteraction,
};

// todo generalize ComponentInteraction and ModalInteraction
#[async_trait::async_trait]
pub trait CreateReplyExt {
    fn attachments(self, attachments: impl IntoIterator<Item = CreateAttachment>) -> Self;
    fn embeds(self, embeds: impl IntoIterator<Item = CreateEmbed>) -> Self;
    async fn respond_to_component(self, ctx: &Context, interaction: &ComponentInteraction) -> Result<()>;
    async fn followup_to_component(self, ctx: &Context, interaction: &ComponentInteraction) -> Result<Message>;
    async fn followup_to_command(self, ctx: &Context, interaction: &CommandInteraction) -> Result<Message>;
    async fn followup_to_modal(self, ctx: &Context, interaction: &ModalInteraction) -> Result<Message>;
    async fn update_to_component(self, ctx: &Context, interaction: &ComponentInteraction) -> Result<()>;
    async fn edit_initial_modal_response(self, ctx: &Context, interaction: &ModalInteraction) -> Result<Message>;
    async fn edit_message(self, ctx: &Context, message: &Message) -> Result<Message>;
}

#[async_trait::async_trait]
impl CreateReplyExt for CreateReply {
    fn attachments(self, attachments: impl IntoIterator<Item = CreateAttachment>) -> Self {
        let mut this = self;
        for attachment in attachments {
            this = this.attachment(attachment);
        }
        this
    }

    fn embeds(self, embeds: impl IntoIterator<Item = CreateEmbed>) -> Self {
        let mut this = self;
        for embed in embeds {
            this = this.embed(embed);
        }
        this
    }

    async fn respond_to_component(self, ctx: &Context, interaction: &ComponentInteraction) -> Result<()> {
        Ok(CreateInteractionResponse::Message(self.to_slash_initial_response(Default::default()))
            .execute(ctx, (interaction.id, &interaction.token))
            .await?)
    }

    async fn followup_to_component(self, ctx: &Context, interaction: &ComponentInteraction) -> Result<Message> {
        Ok(self.to_slash_followup_response(Default::default()).execute(ctx, (None, &interaction.token)).await?)
    }

    async fn followup_to_command(self, ctx: &Context, interaction: &CommandInteraction) -> Result<Message> {
        Ok(self.to_slash_followup_response(Default::default()).execute(ctx, (None, &interaction.token)).await?)
    }

    async fn followup_to_modal(self, ctx: &Context, interaction: &ModalInteraction) -> Result<Message> {
        Ok(self.to_slash_followup_response(Default::default()).execute(ctx, (None, &interaction.token)).await?)
    }

    async fn update_to_component(self, ctx: &Context, interaction: &ComponentInteraction) -> Result<()> {
        Ok(CreateInteractionResponse::UpdateMessage(self.to_slash_initial_response(Default::default()))
            .execute(ctx, (interaction.id, &interaction.token))
            .await?)
    }

    async fn edit_initial_modal_response(self, ctx: &Context, interaction: &ModalInteraction) -> Result<Message> {
        Ok(self.to_slash_initial_response_edit(Default::default()).execute(ctx, &interaction.token).await?)
    }

    async fn edit_message(self, ctx: &Context, message: &Message) -> Result<Message> {
        Ok(self
            .to_prefix_edit(Default::default())
            .execute(ctx, (message.channel_id, message.id, Some(message.author.id)))
            .await?)
    }
}
