public class AsyncFutureService {
    @future
    public static void createInvoice(String status) {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'Future';
        invoice.Amount__c = 20;
        invoice.Status__c = status;
        insert invoice;
    }
}
