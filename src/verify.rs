<<<<<<< HEAD
pub mod captcha;
=======
use std::time::{Duration, Instant};

use dashmap::DashMap;
use once_cell::sync::Lazy;
use poise::serenity_prelude as serenity;
use rand::seq::SliceRandom;
use rand::Rng;

use crate::{Context, Data, Error};

const START_ID: &str = "captcha:start";
const ANSWER_PREFIX: &str = "captcha:ans:";
const TIME_LIMIT: Duration = Duration::from_secs(20);

// image
const GUIDE_IMAGE_URL: &str =
    "https://r2.e-z.host/3d3d3396-6de1-4b53-9dfa-80964810a301/l5er5xu6.png";
const FOOTER_ICON_URL: &str =
    "https://r2.e-z.host/3d3d3396-6de1-4b53-9dfa-80964810a301/5nt79rj0.png";

// color
const COLOR_AQUA: u32 = 0x8FD3FF;
const COLOR_WHITE: u32 = 0xF5FAFF;
const COLOR_FAIL: u32 = 0x9DB7C7;

#[derive(Clone)]
struct Challenge {
    correct: u32,
    expires_at: Instant,
}

static CHALLENGES: Lazy<DashMap<serenity::UserId, Challenge>> = Lazy::new(DashMap::new);

fn verify_role_id(data: &Data) -> serenity::RoleId {
    serenity::RoleId::new(data.config.roles.verify)
}

fn staff_role_id(data: &Data) -> serenity::RoleId {
    serenity::RoleId::new(data.config.roles.staff)
}

async fn is_staff(ctx: Context<'_>) -> Result<bool, Error> {
    let Some(member) = ctx.author_member().await else {
        return Ok(false);
    };
    Ok(member.roles.contains(&staff_role_id(ctx.data())))
}

/// 認証パネルを設置
#[poise::command(slash_command, guild_only, rename = "captcha")]
pub async fn captcha_command(ctx: Context<'_>) -> Result<(), Error> {
    if !is_staff(ctx).await? {
        ctx.send(
            poise::CreateReply::default()
                .content("権限がありません。")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    let embed = serenity::CreateEmbed::new()
        .color(COLOR_AQUA)
        .image(GUIDE_IMAGE_URL);

    let button = serenity::CreateButton::new(START_ID)
        .label("認証する")
        .style(serenity::ButtonStyle::Success);

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .components(vec![serenity::CreateActionRow::Buttons(vec![button])]),
    )
    .await?;

    Ok(())
}

pub async fn handle_component(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let id = interaction.data.custom_id.as_str();

    if id == START_ID {
        return on_start(ctx, interaction).await;
    }
    if let Some(rest) = id.strip_prefix(ANSWER_PREFIX) {
        return on_answer(ctx, data, interaction, rest).await;
    }
    Ok(())
}

pub mod captcha {
    pub use super::captcha_command as captcha;
    pub use super::handle_component;
}

async fn on_start(
    ctx: &serenity::Context,
    interaction: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let user_id = interaction.user.id;

    if let Some(existing) = CHALLENGES.get(&user_id) {
        if Instant::now() <= existing.expires_at {
            interaction
                .create_response(
                    ctx,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("すでに挑戦中です。")
                            .ephemeral(true),
                    ),
                )
                .await?;
            return Ok(());
        }
        CHALLENGES.remove(&user_id);
    }

    let (a, b, correct, mut choices) = {
        let mut rng = rand::thread_rng();
        let a = rng.gen_range(2..=9);
        let b = rng.gen_range(2..=9);
        let correct = a * b;

        let mut choices = vec![correct];
        while choices.len() < 5 {
            let d = rng.gen_range(2..=81);
            if !choices.contains(&d) {
                choices.push(d);
            }
        }
        choices.shuffle(&mut rng);
        (a, b, correct, choices)
    };

    CHALLENGES.insert(
        user_id,
        Challenge {
            correct,
            expires_at: Instant::now() + TIME_LIMIT,
        },
    );

    let embed = serenity::CreateEmbed::new()
        .color(COLOR_WHITE)
        .title("認証チャレンジ")
        .description(format!("**{a} × {b} = ?**"))
        .footer(serenity::CreateEmbedFooter::new("制限時間：20秒"));

    let buttons = choices
        .drain(..)
        .map(|n| {
            serenity::CreateButton::new(format!("{ANSWER_PREFIX}{n}"))
                .label(n.to_string())
                .style(serenity::ButtonStyle::Secondary)
        })
        .collect();

    interaction
        .create_response(
            ctx,
            serenity::CreateInteractionResponse::Message(
                serenity::CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .components(vec![serenity::CreateActionRow::Buttons(buttons)])
                    .ephemeral(true),
            ),
        )
        .await?;

    Ok(())
}

async fn on_answer(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &serenity::ComponentInteraction,
    answered_str: &str,
) -> Result<(), Error> {
    let user_id = interaction.user.id;
    let answered: u32 = match answered_str.parse() {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };

    let Some(ch) = CHALLENGES.get(&user_id).map(|v| v.clone()) else {
        return Ok(());
    };

    if Instant::now() > ch.expires_at {
        CHALLENGES.remove(&user_id);

        let embed = serenity::CreateEmbed::new()
            .color(COLOR_FAIL)
            .title("⌛ 時間切れ")
            .description("もう一度やり直してください。")
            .footer(serenity::CreateEmbedFooter::new("Ayanamist System").icon_url(FOOTER_ICON_URL));

        interaction
            .create_response(
                ctx,
                serenity::CreateInteractionResponse::Message(
                    serenity::CreateInteractionResponseMessage::new()
                        .embed(embed)
                        .ephemeral(true),
                ),
            )
            .await?;
        return Ok(());
    }

    if answered != ch.correct {
        CHALLENGES.remove(&user_id);

        let embed = serenity::CreateEmbed::new()
            .color(COLOR_FAIL)
            .title("❌ 不正解")
            .description("もう一度やり直してください。")
            .footer(serenity::CreateEmbedFooter::new("Ayanamist System").icon_url(FOOTER_ICON_URL));

        interaction
            .create_response(
                ctx,
                serenity::CreateInteractionResponse::Message(
                    serenity::CreateInteractionResponseMessage::new()
                        .embed(embed)
                        .ephemeral(true),
                ),
            )
            .await?;
        return Ok(());
    }

    let Some(guild_id) = interaction.guild_id else {
        return Ok(());
    };

    let member = guild_id.member(ctx, user_id).await?;
    member.add_role(ctx, verify_role_id(data)).await?;
    CHALLENGES.remove(&user_id);

    let embed = serenity::CreateEmbed::new()
        .color(COLOR_AQUA)
        .title("✅ 認証成功")
        .description("ロールを付与しました。")
        .footer(serenity::CreateEmbedFooter::new("Ayanamist System").icon_url(FOOTER_ICON_URL));

    interaction
        .create_response(
            ctx,
            serenity::CreateInteractionResponse::Message(
                serenity::CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .ephemeral(true),
            ),
        )
        .await?;

    Ok(())
}
>>>>>>> be6c202 (Interaction fix)
