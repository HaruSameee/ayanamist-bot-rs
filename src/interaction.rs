use poise::serenity_prelude as serenity;

use crate::Error;

pub async fn respond_component(
    ctx: &serenity::Context,
    interaction: &serenity::ComponentInteraction,
    response: serenity::CreateInteractionResponse,
) -> Result<(), Error> {
    match interaction.create_response(ctx, response).await {
        Ok(()) => Ok(()),
        Err(err) => {
            let message = err.to_string();
            if message.contains("Unknown interaction")
                || message.contains("Interaction has already been acknowledged")
            {
                tracing::debug!("Skipped interaction response: {}", message);
                Ok(())
            } else {
                Err(err.into())
            }
        }
    }
}
