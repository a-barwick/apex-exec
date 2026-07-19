trigger M24ResultTrigger on M24Result__c (before insert, after insert) {
    if (Trigger.isBefore) {
        M24DmlAudit.beforeRows += Trigger.size;
        for (M24Result__c row : Trigger.new) {
            row.Amount__c += 1;
        }
    } else {
        M24DmlAudit.afterRows += Trigger.size;
    }
}
