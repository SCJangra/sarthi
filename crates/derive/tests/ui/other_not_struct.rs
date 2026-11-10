use sarthi_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
enum Event {
	Other(String, rmpv::Value),
}

fn main() {}
