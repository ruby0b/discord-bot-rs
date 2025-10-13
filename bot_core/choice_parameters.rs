use poise::ChoiceParameter;
use poise::serenity_prelude::ButtonStyle;

#[derive(ChoiceParameter)]
pub enum ButtonStyleParameter {
    Primary,
    Secondary,
    Success,
    Danger,
}

impl From<ButtonStyleParameter> for ButtonStyle {
    fn from(value: ButtonStyleParameter) -> Self {
        match value {
            ButtonStyleParameter::Primary => Self::Primary,
            ButtonStyleParameter::Secondary => Self::Secondary,
            ButtonStyleParameter::Success => Self::Success,
            ButtonStyleParameter::Danger => Self::Danger,
        }
    }
}
