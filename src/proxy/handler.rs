use crate::interaction::respond_component;
use crate::{Data, Error};
use poise::serenity_prelude as serenity;
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

struct ProxyInfo {
    address: String,
    typ: String,
}

const BUTTON_DEFINES: &[(&str, &str)] = &[
    ("all", "All"),
    ("http", "HTTP(s)"),
    ("socks4", "Socks4"),
    ("socks5", "Socks5"),
    ("scheme", "All (+Scheme)"),
];

static EMBED_PROXY_SEP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\|\s*").unwrap());

fn read_embed_proxies(m: &serenity::Message) -> Option<Vec<ProxyInfo>> {
    let lines = m.embeds.first().and_then(|e| e.description.as_ref())?;
    let proxies: Vec<_> = lines
        .lines()
        .filter_map(|l| {
            let parts: Vec<&str> = EMBED_PROXY_SEP.split(l).collect();
            let (Some(address), Some(typ)) = (if parts.len() > 2 {
                (parts.first(), parts.get(2))
            } else {
                (parts.first(), parts.get(1))
            }) else {
                return None;
            };

            Some(ProxyInfo {
                address: address.to_string(),
                typ: typ.to_string(),
            })
        })
        .collect();

    (!proxies.is_empty()).then_some(proxies)
}

async fn handle_download_start(
    ctx: &serenity::Context,
    i: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    if read_embed_proxies(&i.message).is_none() {
        respond_component(
            ctx,
            i,
            serenity::CreateInteractionResponse::Message(
                serenity::CreateInteractionResponseMessage::new()
                    .content("このダウンロードに関連付けられたプロキシが取得できません")
                    .ephemeral(true),
            ),
        )
        .await?;

        return Ok(());
    };

    let buttons: Vec<serenity::CreateButton> = BUTTON_DEFINES
        .iter()
        .map(|(id, label)| {
            serenity::CreateButton::new(format!("proxy:download:{}", id))
                .label(label.to_string())
                .style(serenity::ButtonStyle::Secondary)
        })
        .collect();
    let row = serenity::CreateActionRow::Buttons(buttons);

    respond_component(
        ctx,
        i,
        serenity::CreateInteractionResponse::Message(
            serenity::CreateInteractionResponseMessage::new()
                .content("Choose Download Type")
                .ephemeral(true)
                .components(vec![row]),
        ),
    )
    .await?;

    Ok(())
}

pub async fn handle_component(
    ctx: &serenity::Context,
    _data: &Data,
    i: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let id = i.data.custom_id.as_str();

    match id {
        "proxy:download_start" => handle_download_start(ctx, i).await,
        id if id.starts_with("proxy:download:") => {
            let Some(typ) = id.strip_prefix("proxy:download:") else {
                respond_component(
                    ctx,
                    i,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("不明なプロキシの種類です")
                            .ephemeral(true),
                    ),
                )
                .await?;

                return Ok(());
            };
            let Some(proxies) = futures::future::OptionFuture::from(
                i.message
                    .message_reference
                    .as_ref()
                    .and_then(|r| r.message_id.as_ref())
                    .map(|id| i.channel_id.message(ctx, id)),
            )
            .await
            .and_then(|r| r.ok())
            .and_then(|m| read_embed_proxies(&m)) else {
                respond_component(
                    ctx,
                    i,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("このダウンロードに関連付けられたプロキシが取得できません")
                            .ephemeral(true),
                    ),
                )
                .await?;

                return Ok(());
            };

            let download_proxies: Vec<Cow<'_, str>> = match typ {
                "http" | "socks4" | "socks5" => proxies
                    .iter()
                    .filter(|p| p.typ == typ)
                    .map(|p| Cow::Borrowed(p.address.as_str()))
                    .collect(),
                "scheme" => proxies
                    .iter()
                    .map(|p| Cow::Owned(format!("{}://{}", p.typ, p.address)))
                    .collect(),
                _ => proxies
                    .iter()
                    .map(|p| Cow::Borrowed(p.address.as_str()))
                    .collect(),
            };
            let attachment = serenity::CreateAttachment::bytes(
                download_proxies.join("\n").as_bytes(),
                // TODO: ファイル名はランダムかタイムスタンプを含む
                "proxies.txt",
            );

            respond_component(
                ctx,
                i,
                serenity::CreateInteractionResponse::Message(
                    serenity::CreateInteractionResponseMessage::new()
                        .content("Complete")
                        .add_file(attachment)
                        .ephemeral(true),
                ),
            )
            .await?;

            Ok(())
        }
        _ => Ok(()),
    }
}
