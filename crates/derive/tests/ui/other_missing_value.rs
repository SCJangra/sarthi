use nvui_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
enum Event {
	Other { method: String, data: rmpv::Value },
}

fn main() {}
