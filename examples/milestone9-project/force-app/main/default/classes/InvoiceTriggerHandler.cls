public class InvoiceTriggerHandler {
    public static void beforeInsert(List<Invoice__c> invoices) {
        for (Invoice__c invoice : invoices) {
            invoice.Status__c = 'New';
        }
    }

    public static void beforeUpdate(
        List<Invoice__c> invoices,
        Map<String, Invoice__c> oldInvoices
    ) {
        for (Invoice__c invoice : invoices) {
            Invoice__c previous = oldInvoices.get(invoice.Id);
            if (invoice.Amount__c > previous.Amount__c) {
                invoice.Status__c = 'Increased';
            } else {
                invoice.Status__c = 'Unchanged';
            }
        }
    }

    public static void beforeUndelete(List<Invoice__c> invoices) {
        for (Invoice__c invoice : invoices) {
            invoice.Status__c = 'Restored';
        }
    }
}
