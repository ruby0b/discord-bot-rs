use poise::serenity_prelude::{ChannelId, VoiceState};

/// A user's voice state change in a guild regarding their current voice channel.
#[derive(Debug, PartialEq, Eq)]
pub enum VoiceChange {
    Join { to: ChannelId },
    Leave { from: ChannelId },
    Move { from: ChannelId, to: ChannelId },
    Stay,
}

impl VoiceChange {
    pub fn new((old, new): (&Option<VoiceState>, &VoiceState)) -> VoiceChange {
        let old_channel_id = old.as_ref().and_then(|old| old.channel_id);
        match (old_channel_id, new.channel_id) {
            (None, Some(to)) => VoiceChange::Join { to },
            (Some(from), None) => VoiceChange::Leave { from },
            (Some(from), Some(to)) if from != to => VoiceChange::Move { from, to },
            (None, None) | (Some(_), Some(_)) => VoiceChange::Stay,
        }
    }
}
