import { Item } from "./types";

export interface Cart { items: Item[]; }

export type Total = number;

export enum Status { Open, Paid }

export function handleCheckout(cart: Cart): Total {
  return cart.items.length;
}

export const applyDiscount = (total: Total): Total => total * 0.9;

export class CartService {
  add(cart: Cart, item: Item): void {
    cart.items.push(item);
  }
}
