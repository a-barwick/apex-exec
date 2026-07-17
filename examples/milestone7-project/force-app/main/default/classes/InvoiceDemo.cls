public class InvoiceDemo {
    public static void run() {
        Invoice__c invoice = new Invoice__c();
        invoice.Amount__c = 125;
        invoice.Status__c = 'Draft';

        SObject dynamicInvoice = invoice;
        dynamicInvoice.put('Status__c', 'Approved');

        System.debug(invoice.Status__c);
        System.debug(dynamicInvoice.get('Amount__c'));
    }
}
