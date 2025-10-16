// SOURCE: Function with missing closing parenthesis
function calculateTotal(
    items: number[],
    discount: number,
    taxRate: number {
    const subtotal = items.reduce((a, b) => a + b, 0);
    const discounted = subtotal * (1 - discount);
    return discounted * (1 + taxRate);
}

export { calculateTotal };
