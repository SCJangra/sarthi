use serde::Deserialize;

/// A UI-related option changed, e.g. `guifont`, or its value when the UI attached (`:help ui-event-option_set`)
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct OptionSetEvent {
	pub name: String,
	pub value: OptionValue,
}

/// The value of an option: Neovim's options are booleans, numbers or strings
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(untagged)]
pub enum OptionValue {
	Bool(bool),
	Number(i64),
	String(String),
}

#[cfg(test)]
mod tests {
	use rmpv::Value;

	use super::{OptionSetEvent, OptionValue};
	use crate::RedrawNotification;

	#[test]
	fn de_option_set() {
		// From Neovim 0.12, at attach and after `:set guifont=…`
		let event = Value::Array(vec![
			"option_set".into(),
			Value::Array(vec!["termguicolors".into(), false.into()]),
			Value::Array(vec!["ttimeoutlen".into(), 50.into()]),
			Value::Array(vec!["guifont".into(), "JetBrains Mono,Symbols Nerd Font:h15:b".into()]),
		]);
		let event: RedrawNotification = rmp_serde::from_slice(&rmp_serde::to_vec(&event).unwrap()).unwrap();

		let option = |name: &str, value| OptionSetEvent { name: name.to_owned(), value };
		let expected = RedrawNotification::OptionSet(vec![
			option("termguicolors", OptionValue::Bool(false)),
			option("ttimeoutlen", OptionValue::Number(50)),
			option("guifont", OptionValue::String(String::from("JetBrains Mono,Symbols Nerd Font:h15:b"))),
		]);
		assert_eq!(event, expected);
	}
}
