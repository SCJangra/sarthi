use sarthi_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
enum Event {
	Other { name: String, value: rmpv::Value },
}

fn main() {}
