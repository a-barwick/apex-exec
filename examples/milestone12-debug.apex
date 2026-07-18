public class PriceCalculator {
    public static Integer discounted(Integer subtotal, Integer percent) {
        Integer discount = subtotal * percent / 100;
        return subtotal - discount;
    }
}

List<Integer> subtotals = new List<Integer>{120, 80};
Integer total = 0;
for (Integer subtotal : subtotals) {
    total = total + PriceCalculator.discounted(subtotal, 10);
}
System.debug(total);
