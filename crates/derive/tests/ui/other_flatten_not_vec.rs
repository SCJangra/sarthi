use nvui_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
enum Event {
	Other {
		method: String,
		#[tagged_enum(flatten)]
		value: u32,
	},
}

fn main() {}
