public class PricingService {
    public static Integer extended(Integer quantity, Integer unitPrice) {
        if (quantity <= 0) {
            return 0;
        }
        return quantity * unitPrice;
    }
}
