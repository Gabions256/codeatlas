import os
from .models import Cart


class Shop:
    def pay(self, cart, tip=0):
        return total(cart) + self.log(tip)

    def log(self, value):
        print(value)
        return value


def total(cart):
    return len(cart)
