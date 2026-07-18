trigger InvoiceOracleTrigger on Invoice__c (before insert, after insert) {
    if (Trigger.isBefore) {
        for (Invoice__c invoice : Trigger.new) {
            invoice.Status__c = 'prepared';
        }
    }
}
