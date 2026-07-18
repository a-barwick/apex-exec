@IsTest
private class CheckoutServiceTest {
    @IsTest
    static void quotesCart() {
        List<Integer> quantities = new List<Integer>{2, 3};
        System.assertEquals(125, CheckoutService.quote(quantities));
    }
}
