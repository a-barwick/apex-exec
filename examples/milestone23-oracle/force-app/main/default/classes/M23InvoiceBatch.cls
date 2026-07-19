public class M23InvoiceBatch implements Database.Batchable<SObject> {
    public Database.QueryLocator start(Database.BatchableContext context) {
        return Database.getQueryLocator(
            'SELECT Id, Status__c FROM M23Invoice__c WHERE Status__c = \'Open\' ORDER BY Name'
        );
    }

    public void execute(
        Database.BatchableContext context,
        List<SObject> scope
    ) {
        for (SObject invoice : scope) {
            invoice.put('Status__c', 'Processed');
        }
        update scope;
    }

    public void finish(Database.BatchableContext context) {}
}
