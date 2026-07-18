@IsTest
private class PricingServiceTest {
    @IsTest
    static void pricesPositiveQuantity() {
        System.assertEquals(75, PricingService.extended(3, 25));
    }

    @IsTest
    static void rejectsNonPositiveQuantity() {
        System.assertEquals(0, PricingService.extended(0, 25));
    }
}
