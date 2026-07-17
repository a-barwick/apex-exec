public class AsyncInvoiceBatch implements Database.Batchable<Invoice__c> {
    public List<Invoice__c> start(Database.BatchableContext context) {
        return [
            SELECT Id, Amount__c, Status__c
            FROM Invoice__c
            ORDER BY Amount__c
        ];
    }

    public void execute(
        Database.BatchableContext context,
        List<Invoice__c> scope
    ) {
        for (Invoice__c invoice : scope) {
            invoice.Status__c = 'batched';
        }
        update scope;
    }

    public void finish(Database.BatchableContext context) {
        System.debug('batch complete');
    }
}
