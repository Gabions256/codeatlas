use std::fmt::Display;

pub struct Cart {
    n: u32,
}

impl Cart {
    pub fn new(n: u32) -> Self {
        check(n);
        Cart { n }
    }

    fn size(&self) -> u32 {
        self.n
    }
}

fn check(n: u32) {
    let cart = Cart::new(n);
    cart.size();
}
