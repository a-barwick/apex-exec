trigger InvoiceTrigger on Invoice__c (
    before insert,
    before update,
    after delete,
    before undelete
) {
    if (Trigger.isInsert) {
        InvoiceTriggerHandler.beforeInsert(Trigger.new);
    } else if (Trigger.isUpdate) {
        InvoiceTriggerHandler.beforeUpdate(Trigger.new, Trigger.oldMap);
    } else if (Trigger.isUndelete) {
        InvoiceTriggerHandler.beforeUndelete(Trigger.new);
    }
}
