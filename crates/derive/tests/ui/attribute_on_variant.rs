use sarthi_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
enum Event {
	#[tagged_enum(flatten)]
	Grid(Vec<u32>),
	Other { method: String, value: rmpv::Value },
}

fn main() {}
