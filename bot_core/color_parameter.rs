use derive_more::Into;
use hex::FromHex as _;
use poise::serenity_prelude::{
    Color, CommandInteraction, CommandOptionType, Context, CreateCommandOption, ResolvedValue,
};
use poise::{SlashArgError, SlashArgument};
use std::str::FromStr;

#[derive(Into)]
pub struct HexColorParameter(Color);

impl FromStr for HexColorParameter {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix('#').unwrap_or(s);
        let color = (|| {
            let red = parse_hex_byte_at(s, 0)?;
            let green = parse_hex_byte_at(s, 2)?;
            let blue = parse_hex_byte_at(s, 4)?;
            Some(Color::from_rgb(red, green, blue))
        })()
        .ok_or("Error while parsing hex color")?;
        Ok(HexColorParameter(color))
    }
}

fn parse_hex_byte_at(hex_str: &str, index: usize) -> Option<u8> {
    let c0 = hex_str.chars().nth(index)?;
    let c1 = hex_str.chars().nth(index + 1)?;
    let cs = [c0, c1].iter().collect::<String>();
    let byte = <[u8; 1]>::from_hex(cs).ok()?;
    Some(byte[0])
}

#[async_trait::async_trait]
impl SlashArgument for HexColorParameter {
    async fn extract(
        _: &Context,
        _: &CommandInteraction,
        value: &ResolvedValue<'_>,
    ) -> Result<HexColorParameter, SlashArgError> {
        match *value {
            ResolvedValue::String(s) => s
                .parse::<HexColorParameter>()
                .map_err(SlashArgError::new_command_structure_mismatch),
            _ => Err(SlashArgError::new_command_structure_mismatch("expected string")),
        }
    }

    fn create(builder: CreateCommandOption) -> CreateCommandOption {
        builder.kind(CommandOptionType::String)
    }
}
