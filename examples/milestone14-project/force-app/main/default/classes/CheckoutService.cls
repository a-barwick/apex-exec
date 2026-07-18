public class CheckoutService {
    public static Integer quote(List<Integer> quantities) {
        Integer total = 0;
        for (Integer quantity : quantities) {
            total = total + PricingService.extended(quantity, 25);
        }
        return total;
    }
}
