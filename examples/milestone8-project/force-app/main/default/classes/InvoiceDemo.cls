public class InvoiceDemo {
    public static void run() {
        Customer__c customer = new Customer__c();
        customer.Name = 'Acme';
        insert customer;

        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'INV-100';
        invoice.Amount__c = 125;
        invoice.Status__c = 'Approved';
        invoice.Customer__c = customer.Id;
        Database.insert(invoice);

        List<Invoice__c> invoices = InvoiceRepository.findHighValue(100);
        System.debug(invoices[0].Name);
        System.debug(invoices[0].Customer__r.Name);
        System.debug([SELECT COUNT() FROM Invoice__c]);
    }
}
