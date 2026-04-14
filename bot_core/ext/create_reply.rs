use eyre::Result;
use poise::CreateReply;
use poise::serenity_prelude::{
    Builder as _, ComponentInteraction, Context, CreateInteractionResponse, Message, ModalInteraction,
};

// todo generalize ComponentInteraction and ModalInteraction
#[async_trait::async_trait]
pub trait CreateReplyExt {
    async fn respond_to_interaction(self, ctx: &Context, interaction: &ComponentInteraction) -> Result<()>;

    async fn edit_interaction(self, ctx: &Context, interaction: &ModalInteraction) -> Result<Message>;

    async fn edit_message(self, ctx: &Context, message: &Message) -> Result<Message>;
}

#[async_trait::async_trait]
impl CreateReplyExt for CreateReply {
    async fn respond_to_interaction(self, ctx: &Context, interaction: &ComponentInteraction) -> Result<()> {
        Ok(CreateInteractionResponse::Message(self.to_slash_initial_response(Default::default()))
            .execute(ctx, (interaction.id, &interaction.token))
            .await?)
    }

    async fn edit_interaction(self, ctx: &Context, interaction: &ModalInteraction) -> Result<Message> {
        Ok(self.to_slash_initial_response_edit(Default::default()).execute(ctx, &interaction.token).await?)
    }

    async fn edit_message(self, ctx: &Context, message: &Message) -> Result<Message> {
        Ok(self
            .to_prefix_edit(Default::default())
            .execute(ctx, (message.channel_id, message.id, Some(message.author.id)))
            .await?)
    }
}
