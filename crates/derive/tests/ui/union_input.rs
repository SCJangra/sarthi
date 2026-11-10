use sarthi_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
union Event {
	grid: u32,
}

fn main() {}
