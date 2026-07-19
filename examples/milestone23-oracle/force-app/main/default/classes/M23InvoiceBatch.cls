public class M23InvoiceBatch implements Database.Batchable<M23Invoice__c> {
    public Database.QueryLocator start(Database.BatchableContext context) {
        return Database.getQueryLocator(
            'SELECT Id, Status__c FROM M23Invoice__c WHERE Status__c = \'Open\' ORDER BY Name'
        );
    }

    public void execute(
        Database.BatchableContext context,
        List<M23Invoice__c> scope
    ) {
        for (M23Invoice__c invoice : scope) {
            invoice.Status__c = 'Processed';
        }
        update scope;
    }

    public void finish(Database.BatchableContext context) {}
}
