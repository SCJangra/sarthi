use nvui_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
enum Event {
	Other {
		#[tagged_enum(flatten)]
		method: Vec<String>,
		value: rmpv::Value,
	},
}

fn main() {}
