use serenity::all::Color;
use serenity::all::CreateEmbed;
use serenity::all::ExecuteWebhook;
use serenity::all::Http;
use serenity::all::Timestamp;
use tracing::Instrument;
use tracing::Level;
use tracing::Subscriber;
use tracing_subscriber::Layer as TracingLayer;

use crate::trimmed_embed::Size;
use crate::trimmed_embed::TrimmedEmbed;

use super::visitor;

type Msg = Box<CreateEmbed>;
type Fields = Vec<(String, String, bool)>;

pub struct Layer {
    channel: tokio::sync::mpsc::Sender<Msg>,
}

impl Layer {
    pub fn build(error_webhook: Option<String>, http: Http) -> Layer {
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<Msg>(50);
        tokio::spawn(async move {
            // Load the webhook
            let maybe_webhook = match error_webhook {
                Some(url) => match serenity::model::webhook::Webhook::from_url(&http, &url).await {
                    Ok(webhook) => Some(webhook),
                    Err(e) => {
                        println!("ERROR: Failed to initialize debug webhook: {:?}", e);
                        None
                    }
                },
                None => None,
            };

            loop {
                let Some(embed) = receiver.recv().await else {
                    break;
                };

                let Some(webhook) = &maybe_webhook else {
                    println!("NO WEBHOOK CONFIG. NOT SENDING DEBUG MESSAGE THROUGH WEBHOOK.");
                    continue;
                };

                let res = webhook
                    .execute(&http, false, ExecuteWebhook::new().embed(*embed))
                    .await;
                if let Err(err) = res {
                    println!("Failed to send debug webhook message: {:?}", err);
                }
            }
        });
        Layer { channel: sender }
    }
}

impl<S: Subscriber> TracingLayer<S> for Layer
where
    S: tracing::Subscriber,
    S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let span = ctx.span(id).unwrap();

        let mut data = vec![];
        data.push((
            "Span".to_owned(),
            attrs.metadata().target().to_owned() + "::" + attrs.metadata().name(),
            false,
        ));
        let mut visitor = visitor::EmbedFieldVisitor {
            fields: data,
            field_name_prefix: Some("Span:".to_owned()),
            ..Default::default()
        };
        attrs.record(&mut visitor);
        span.extensions_mut().insert::<Fields>(visitor.fields);
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let level = event.metadata().level();
        if *level > Level::WARN {
            return;
        }

        let color = match *level {
            Level::ERROR => Color::from_rgb(255, 0, 0),
            Level::WARN => Color::from_rgb(255, 255, 0),
            _ => Color::from_rgb(0, 0, 0),
        };

        let file = event.metadata().file().unwrap_or("Unknown");
        let line = event
            .metadata()
            .line()
            .map(|i| i.to_string())
            .unwrap_or_else(|| "Unknown".to_owned());

        let span_fields = if let Some(span_id) = event.metadata().in_current_span().span().id() {
            ctx.span_scope(&span_id)
                .map(|scope| {
                    scope
                        .into_iter()
                        .flat_map(|s| {
                            s.extensions()
                                .get::<Fields>()
                                .map(|fields| fields.clone())
                                .unwrap_or_else(|| vec![])
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec![])
        } else {
            vec![]
        };

        let mut visitor = visitor::EmbedFieldVisitor::default();
        event.record(&mut visitor);

        let mut size = Size::new();
        let embed = TrimmedEmbed::new(&mut size)
            .title(level.to_string().to_uppercase())
            .description(visitor.message.unwrap_or_else(|| "No message".to_owned()))
            .timestamp(Timestamp::now())
            .color(color)
            .field("File", file, true)
            .field("Line", line, true)
            .field("Target", event.metadata().target(), true)
            .fields(
                visitor
                    .fields
                    .into_iter()
                    .chain(span_fields.into_iter())
                    .take(22),
            );
        if let Err(err) = self.channel.try_send(Box::new(embed.into())) {
            tracing::error!(err = %err, "failed to send discord payload to given channel");
        }
    }
}
