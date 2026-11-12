import { handleCheckout, applyDiscount, Cart } from "./cart";

export function payNow(cart: Cart): number {
  const total = handleCheckout(cart);
  console.log(total);
  return applyDiscount(total);
}
