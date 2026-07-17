@IsTest
private class InvoiceTriggerTest {
    @IsTest
    static void demoEntryRuns() {
        TriggerDemo.run();
        System.assertEquals(1, [SELECT COUNT() FROM Invoice__c]);
    }

    @IsTest
    static void handlesInsertUpdateDeleteAndUndelete() {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'Test';
        invoice.Amount__c = 10;
        insert invoice;

        invoice.Amount__c = 20;
        update invoice;
        Invoice__c increased = [
            SELECT Id, Status__c FROM Invoice__c WHERE Id = :invoice.Id LIMIT 1
        ];
        System.assertEquals('Increased', increased.Status__c);

        delete invoice;
        System.assertEquals(0, [SELECT COUNT() FROM Invoice__c]);
        undelete invoice;
        Invoice__c restored = [
            SELECT Id, Status__c FROM Invoice__c WHERE Id = :invoice.Id LIMIT 1
        ];
        System.assertEquals('Restored', restored.Status__c);
    }

    @IsTest
    static void handlesNonIncreasingUpdate() {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'Stable';
        invoice.Amount__c = 10;
        insert invoice;
        update invoice;
        Invoice__c unchanged = [
            SELECT Id, Status__c FROM Invoice__c WHERE Id = :invoice.Id LIMIT 1
        ];
        System.assertEquals('Unchanged', unchanged.Status__c);
    }
}
