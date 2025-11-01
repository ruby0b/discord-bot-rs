use crate::{DECLINE_BUTTON_ID, JOIN_BUTTON_ID, LEAVE_BUTTON_ID};
use chrono::{DateTime, TimeDelta, Utc};
use poise::serenity_prelude::{
    ButtonStyle, ChannelId, Colour, CreateActionRow, CreateAllowedMentions, CreateButton,
    CreateEmbed, CreateMessage, EditMessage, Mentionable as _, MessageId, RoleId, UserId,
};
use url::Url;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ask {
    pub players: Vec<UserId>,
    pub declined_players: Vec<UserId>,
    pub min_players: Option<u32>,
    pub max_players: Option<u32>,
    pub title: String,
    pub url: Option<Url>,
    pub description: Option<String>,
    pub thumbnail_url: Option<String>,
    pub channel_id: ChannelId,
    pub role_id: Option<RoleId>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub start_time: DateTime<Utc>,
    pub pinged: bool,
}

impl Ask {
    pub(crate) fn edit_message(&self) -> EditMessage {
        EditMessage::new()
            .content(self.content())
            .embed(self.embed())
            .allowed_mentions(CreateAllowedMentions::new().roles(self.role_id))
    }

    pub(crate) fn content(&self) -> String {
        self.role_id.map(|r| r.mention().to_string()).unwrap_or_default()
    }

    pub(crate) fn embed(&self) -> CreateEmbed {
        let min = self.min_players.map(|x| x.to_string()).unwrap_or("0".to_string());
        let max = self.max_players.map(|x| x.to_string()).unwrap_or("∞".to_string());

        let embed = CreateEmbed::default().title(self.title.clone());
        let embed = embed.colour(if self.full() {
            Colour::BLUE
        } else if self.start_time > Utc::now() {
            Colour::GOLD
        } else {
            Colour::DARK_GREEN
        });
        let embed = embed.field("Min Players", min, true);
        let embed = embed.field("Max Players", max, true);
        let embed = embed.fields((!self.has_started()).then(|| {
            let unix = self.start_time.timestamp();
            ("Starts", format!("<t:{unix}:R>"), true)
        }));
        let embed = embed.fields((!self.declined_players.is_empty()).then(|| {
            let mentions = user_mentions(&self.declined_players);
            ("Declined", format!("-# {mentions}"), false)
        }));
        let embed = embed.field(
            format!("Players: {}", self.players.len()),
            user_mentions(&self.players),
            false,
        );
        let embed = match &self.description {
            Some(description) => embed.description(description),
            None => embed,
        };
        let embed = match &self.url {
            Some(url) => embed.url(url.clone()),
            None => embed,
        };
        let embed = match &self.thumbnail_url {
            Some(url) => embed.thumbnail(url.clone()),
            None => embed,
        };
        embed
    }

    pub(crate) fn full(&self) -> bool {
        self.max_players.is_some_and(|x| x as usize == self.players.len())
    }

    fn has_started(&self) -> bool {
        let delta = self.start_time.signed_duration_since(Utc::now());
        delta < TimeDelta::seconds(3)
    }

    pub(crate) fn action_row(&self) -> CreateActionRow {
        CreateActionRow::Buttons(vec![
            CreateButton::new(JOIN_BUTTON_ID)
                .style(ButtonStyle::Success)
                .disabled(self.full())
                .label("Join"),
            CreateButton::new(DECLINE_BUTTON_ID).style(ButtonStyle::Danger).label("Decline"),
            CreateButton::new(LEAVE_BUTTON_ID).style(ButtonStyle::Secondary).label("Leave"),
        ])
    }

    pub(crate) fn ping(&mut self, msg_id: MessageId) -> Option<CreateMessage> {
        (!self.pinged
            && self.has_started()
            && self.players.len() >= self.min_players.unwrap_or(u32::MAX) as usize)
            .then(|| {
                self.pinged = true;
                CreateMessage::new().reference_message((self.channel_id, msg_id)).content(format!(
                    "**Lobby readyyyyy!!!!!!!!**\n-# {}",
                    user_mentions(&self.players)
                ))
            })
    }
}

fn user_mentions(user_ids: &[UserId]) -> String {
    user_ids.iter().map(|p| p.mention().to_string()).collect::<Vec<_>>().join(" ")
}
