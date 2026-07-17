public class TriggerDemo {
    public static void run() {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'INV-900';
        invoice.Amount__c = 100;
        insert invoice;

        invoice.Amount__c = 125;
        update invoice;
        Invoice__c increased = [
            SELECT Id, Status__c FROM Invoice__c WHERE Id = :invoice.Id LIMIT 1
        ];
        System.debug(increased.Status__c);

        delete invoice;
        undelete invoice;
        Invoice__c restored = [
            SELECT Id, Status__c FROM Invoice__c WHERE Id = :invoice.Id LIMIT 1
        ];
        System.debug(restored.Status__c);
        System.debug([SELECT COUNT() FROM Invoice__c]);
    }
}
