use crate::{DECLINE_BUTTON_ID, JOIN_BUTTON_ID, LEAVE_BUTTON_ID, TOGGLE_GAME_ROLE_BUTTON_ID};
use chrono::{DateTime, TimeDelta, Utc};
use itertools::Itertools;
use poise::serenity_prelude::{
    ButtonStyle, ChannelId, Colour, CreateActionRow, CreateAllowedMentions, CreateButton, CreateEmbed, CreateMessage,
    EditMessage, Mentionable as _, MessageId, RoleId, UserId,
};
use std::collections::BTreeMap;
use url::Url;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ask {
    pub players: BTreeMap<UserId, AskPlayer>,
    pub min_players: Option<u32>,
    pub max_players: Option<u32>,
    pub title: String,
    pub url: Option<Url>,
    pub description: Option<String>,
    pub thumbnail_url: Option<String>,
    pub channel_id: ChannelId,
    pub role_id: AskRoleId,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub start_time: DateTime<Utc>,
    pub pinged: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AskPlayer {
    pub entered_at: DateTime<Utc>,
    pub state: AskPlayerState,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) enum AskPlayerState {
    Declined,
    Joined,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AskRoleId {
    KnownGame(RoleId),
    Other(RoleId),
    None,
}

impl AskRoleId {
    pub(crate) fn into_option(self) -> Option<RoleId> {
        match self {
            AskRoleId::KnownGame(id) => Some(id),
            AskRoleId::Other(id) => Some(id),
            AskRoleId::None => None,
        }
    }
}

impl Ask {
    pub(crate) fn edit_message(&self) -> EditMessage {
        EditMessage::new()
            .content(self.content())
            .embed(self.embed())
            .allowed_mentions(CreateAllowedMentions::new().roles(self.role_id.into_option()))
    }

    pub(crate) fn content(&self) -> String {
        self.role_id.into_option().map(|r| r.mention().to_string()).unwrap_or_default()
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
        let embed = {
            let declined = self.declined_players().collect_vec();
            embed.fields((!declined.is_empty()).then(|| ("Declined", user_mentions(declined), false)))
        };
        let embed = {
            let queued = self.queued_players().collect_vec();
            embed.fields((!queued.is_empty()).then(|| ("In Queue", user_mentions(queued), false)))
        };
        let embed = {
            let joined = self.joined_players().collect_vec();
            embed.field(format!("Players: {}", joined.len()), user_mentions(joined), false)
        };
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

    fn declined_players(&self) -> impl Iterator<Item = UserId> {
        self.players
            .iter()
            .filter(|(_, p)| p.state == AskPlayerState::Declined)
            .sorted_by_key(|(_, p)| p.entered_at)
            .map(|(id, _)| *id)
    }

    fn queued_players(&self) -> impl Iterator<Item = UserId> {
        self.all_ready_players().skip(self.max_players.map_or(usize::MAX, |n| n as usize))
    }

    fn joined_players(&self) -> impl Iterator<Item = UserId> {
        self.all_ready_players().take(self.max_players.map_or(usize::MAX, |n| n as usize))
    }

    fn all_ready_players(&self) -> impl Iterator<Item = UserId> {
        self.players
            .iter()
            .filter_map(|(id, p)| match p.state {
                AskPlayerState::Joined => Some((p.entered_at, id)),
                _ => None,
            })
            .sorted() // sort by join time (and secondarily by id to ensure stability in case of ties)
            .map(|(_, &id)| id)
    }

    pub(crate) fn full(&self) -> bool {
        self.max_players.is_some_and(|x| x as usize == self.joined_players().count())
    }

    fn has_started(&self) -> bool {
        let delta = self.start_time.signed_duration_since(Utc::now());
        delta < TimeDelta::seconds(3)
    }

    pub(crate) fn action_row(&self) -> CreateActionRow {
        let mut buttons = vec![
            CreateButton::new(JOIN_BUTTON_ID).style(ButtonStyle::Success).label("Join"),
            CreateButton::new(DECLINE_BUTTON_ID).style(ButtonStyle::Danger).label("Decline"),
            CreateButton::new(LEAVE_BUTTON_ID).style(ButtonStyle::Secondary).label("Leave"),
        ];
        if let AskRoleId::KnownGame(_) = self.role_id {
            buttons.push(CreateButton::new(TOGGLE_GAME_ROLE_BUTTON_ID).style(ButtonStyle::Secondary).emoji('🔔'));
        }
        CreateActionRow::Buttons(buttons)
    }

    pub(crate) fn ping(&mut self, msg_id: MessageId) -> Option<CreateMessage> {
        if self.pinged || !self.has_started() {
            return None;
        }
        let ready_players = self.all_ready_players().collect_vec();
        if ready_players.len() < self.min_players.unwrap_or(u32::MAX) as usize {
            return None;
        }

        self.pinged = true;
        Some(
            CreateMessage::new()
                .reference_message((self.channel_id, msg_id))
                .content(format!("**Lobby ready!**\n-# {}", user_mentions(ready_players))),
        )
    }
}

fn user_mentions(user_ids: impl IntoIterator<Item = UserId>) -> String {
    user_ids.into_iter().map(|p| p.mention().to_string()).collect::<Vec<_>>().join(" ")
}
