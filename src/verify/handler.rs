use crate::image::encode_webp;
use crate::verify::captcha::{generate_answer, render_captcha};
use crate::verify::common::{COLOR_AQUA, COLOR_FAIL, COLOR_WHITE, FOOTER_ICON_URL};
use crate::verify::state::{Challenge, FailureTracker, SubmitOutcome};
use crate::{Data, Error, db};
use dashmap::DashMap;
use poise::serenity_prelude as serenity;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

pub const START_ID: &str = "captcha:start";
pub const ANSWER_ID: &str = "captcha:answer";
pub const SUBMIT_ID: &str = "captcha:submit";
const INPUT_ID: &str = "captcha:input";
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

static CHALLENGES: LazyLock<DashMap<serenity::UserId, Challenge>> = LazyLock::new(DashMap::new);
static FAILURES: LazyLock<DashMap<serenity::UserId, FailureTracker>> = LazyLock::new(DashMap::new);

/// 期限切れのチャレンジとアイドルな失敗トラッカーを定期的に削除する。
pub async fn cleanup_task() {
    let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let now = Instant::now();
        CHALLENGES.retain(|_, c| !c.is_expired(now));
        FAILURES.retain(|_, f| !f.is_idle(now));
    }
}

fn system_footer() -> serenity::CreateEmbedFooter {
    serenity::CreateEmbedFooter::new("Ayanamist System").icon_url(FOOTER_ICON_URL)
}

/// 認証結果を DB に記録する。失敗しても認証フローは継続する。
async fn record_verify_log(data: &Data, user_id: serenity::UserId, result: &'static str) {
    if let Err(err) = db::insert_verify_log(&data.db, user_id.get(), result, db::now_unix()).await {
        tracing::error!("failed to record verify log: {err}");
    }
}

fn ephemeral_response(
    f: impl FnOnce(
        serenity::CreateInteractionResponseMessage,
    ) -> serenity::CreateInteractionResponseMessage,
) -> serenity::CreateInteractionResponse {
    serenity::CreateInteractionResponse::Message(
        f(serenity::CreateInteractionResponseMessage::new()).ephemeral(true),
    )
}

pub async fn handle_component(
    ctx: &serenity::Context,
    _data: &Data,
    interaction: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    match interaction.data.custom_id.as_str() {
        START_ID => on_start(ctx, interaction).await,
        ANSWER_ID => on_answer_open(ctx, interaction).await,
        _ => Ok(()),
    }
}

async fn on_start(
    ctx: &serenity::Context,
    interaction: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let user_id = interaction.user.id;
    let now = Instant::now();

    if let Some(tracker) = FAILURES.get(&user_id)
        && tracker.is_in_cooldown(now)
    {
        interaction
            .create_response(
                ctx,
                ephemeral_response(|m| {
                    m.content(
                        "連続して失敗したため、認証は一時的に制限されています。しばらく時間をおいてから再度お試しください。",
                    )
                }),
            )
            .await?;
        return Ok(());
    }

    if let Some(existing) = CHALLENGES.get(&user_id)
        && !existing.is_expired(now)
    {
        interaction
            .create_response(ctx, ephemeral_response(|m| m.content("すでに挑戦中です。")))
            .await?;
        return Ok(());
    }
    CHALLENGES.remove(&user_id);

    let (answer, webp) = tokio::task::spawn_blocking(|| {
        let mut rng = rand::thread_rng();
        let answer = generate_answer(&mut rng);
        render_captcha(&mut rng, &answer)
            .and_then(|img| encode_webp(&img))
            .map(|bytes| (answer, bytes))
    })
    .await??;

    CHALLENGES.insert(user_id, Challenge::new(answer, now));

    let embed = serenity::CreateEmbed::new()
        .color(COLOR_WHITE)
        .title("認証チャレンジ")
        .description("画像に表示されている英数字を入力してください。")
        .footer(serenity::CreateEmbedFooter::new("制限時間：120秒"));

    let button = serenity::CreateButton::new(ANSWER_ID)
        .label("回答する")
        .style(serenity::ButtonStyle::Primary);

    interaction
        .create_response(
            ctx,
            serenity::CreateInteractionResponse::Message(
                serenity::CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .add_file(serenity::CreateAttachment::bytes(webp, "captcha.webp"))
                    .components(vec![serenity::CreateActionRow::Buttons(vec![button])])
                    .ephemeral(true),
            ),
        )
        .await?;

    Ok(())
}

async fn on_answer_open(
    ctx: &serenity::Context,
    interaction: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let user_id = interaction.user.id;
    let now = Instant::now();
    let valid = CHALLENGES.get(&user_id).is_some_and(|c| !c.is_expired(now));

    if !valid {
        interaction
            .create_response(
                ctx,
                ephemeral_response(|m| {
                    m.content("チャレンジが見つかりません。もう一度「認証する」を押してください。")
                }),
            )
            .await?;
        return Ok(());
    }

    let modal = serenity::CreateModal::new(SUBMIT_ID, "認証").components(vec![
        serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(serenity::InputTextStyle::Short, "答え", INPUT_ID)
                .placeholder("画像に表示されている英数字"),
        ),
    ]);
    interaction
        .create_response(ctx, serenity::CreateInteractionResponse::Modal(modal))
        .await?;

    Ok(())
}

pub async fn handle_modal(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &serenity::ModalInteraction,
) -> Result<(), Error> {
    if interaction.data.custom_id != SUBMIT_ID {
        return Ok(());
    }

    let user_id = interaction.user.id;
    let now = Instant::now();

    let input = interaction
        .data
        .components
        .iter()
        .flat_map(|row| row.components.iter())
        .find_map(|c| match c {
            serenity::ActionRowComponent::InputText(t) if t.custom_id == INPUT_ID => {
                t.value.clone()
            }
            _ => None,
        });
    let Some(input) = input else {
        return Ok(());
    };

    let Some((_, mut challenge)) = CHALLENGES.remove(&user_id) else {
        interaction
            .create_response(
                ctx,
                ephemeral_response(|m| {
                    m.content("チャレンジが見つかりません。もう一度「認証する」を押してください。")
                }),
            )
            .await?;
        return Ok(());
    };

    match challenge.submit(&input, now) {
        SubmitOutcome::Correct => {
            record_verify_log(data, user_id, "success").await;
            let Some(guild_id) = interaction.guild_id else {
                return Ok(());
            };
            let member = guild_id.member(ctx, user_id).await?;
            member
                .add_role(ctx, data.config.verify.verify_role_id)
                .await?;

            let embed = serenity::CreateEmbed::new()
                .color(COLOR_AQUA)
                .title("✅ 認証成功")
                .description("ロールを付与しました。")
                .footer(system_footer());
            interaction
                .create_response(ctx, ephemeral_response(|m| m.embed(embed)))
                .await?;
        }
        SubmitOutcome::Wrong { invalidated } => {
            record_verify_log(data, user_id, "fail").await;
            FAILURES.entry(user_id).or_default().record_failure(now);
            if !invalidated {
                // 試行回数が残っているのでチャレンジを継続する
                CHALLENGES.insert(user_id, challenge);
            }
            let embed = serenity::CreateEmbed::new()
                .color(COLOR_FAIL)
                .title("❌ 不正解")
                .description("もう一度やり直してください。")
                .footer(system_footer());
            interaction
                .create_response(ctx, ephemeral_response(|m| m.embed(embed)))
                .await?;
        }
        SubmitOutcome::Expired => {
            record_verify_log(data, user_id, "timeout").await;
            FAILURES.entry(user_id).or_default().record_failure(now);
            let embed = serenity::CreateEmbed::new()
                .color(COLOR_FAIL)
                .title("⌛ 時間切れ")
                .description("もう一度やり直してください。")
                .footer(system_footer());
            interaction
                .create_response(ctx, ephemeral_response(|m| m.embed(embed)))
                .await?;
        }
    }

    Ok(())
}
