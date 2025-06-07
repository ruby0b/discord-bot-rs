use poise::serenity_prelude::all::{ChannelId, GuildId, MessageId, parse_message_url};

pub mod td_seconds {
    use chrono::TimeDelta;
    use serde::{Deserialize, Serialize, de, ser};

    pub fn serialize<S>(value: &TimeDelta, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        value.num_seconds().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<TimeDelta, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        Ok(TimeDelta::seconds(<i64 as Deserialize>::deserialize(deserializer)?))
    }
}

pub mod duration_seconds {
    use serde::{Deserialize, Serialize, de, ser};
    use std::time::Duration;

    pub fn serialize<S>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        value.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        Ok(Duration::from_secs(<u64 as Deserialize>::deserialize(deserializer)?))
    }
}

pub mod emoji {
    use poise::serenity_prelude::all::ReactionType;
    use serde::{Deserialize, Serialize, de, ser};

    pub fn serialize<S>(value: &ReactionType, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        value.to_string().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ReactionType, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        ReactionType::try_from(<String as Deserialize>::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}

pub mod regex_str {
    use fancy_regex::Regex;
    use serde::{Deserialize, Serialize, de, ser};

    pub fn serialize<S>(value: &Regex, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        value.as_str().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Regex, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        Regex::new(&<String as Deserialize>::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MessageLink {
    pub guild_id: GuildId,
    pub channel_id: ChannelId,
    pub message_id: MessageId,
}

impl std::fmt::Display for MessageLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message_id.link(self.channel_id, Some(self.guild_id)))
    }
}

impl serde::Serialize for MessageLink {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for MessageLink {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let url = String::deserialize(deserializer)?;
        let (guild_id, channel_id, message_id) = parse_message_url(&url)
            .ok_or_else(|| serde::de::Error::custom("Invalid message URL"))?;
        Ok(MessageLink { message_id, channel_id, guild_id })
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct LiteralRegex(#[serde(with = "regex_str")] pub fancy_regex::Regex);

impl PartialEq for LiteralRegex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Eq for LiteralRegex {}

impl PartialOrd for LiteralRegex {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LiteralRegex {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_str().cmp(other.0.as_str())
    }
}
