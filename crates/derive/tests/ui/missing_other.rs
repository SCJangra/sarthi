use sarthi_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
enum Event {
	Flush,
}

fn main() {}
