use sarthi_derive::DeserializeTaggedEnum;

struct NotDeserialize;

#[derive(DeserializeTaggedEnum)]
enum Event {
	Grid(u32, NotDeserialize),
	Other { method: String, value: rmpv::Value },
}

fn main() {}
