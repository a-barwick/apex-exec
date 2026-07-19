trigger M24ResultTrigger on M24Result__c (before insert, after insert) {
    if (Trigger.isBefore) {
        M24DmlAudit.beforeRows += Trigger.size;
    } else {
        M24DmlAudit.afterRows += Trigger.size;
    }
}
