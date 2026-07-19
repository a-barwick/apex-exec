trigger M25LegacyTrigger on M25Profile__c (before insert) {
    Object value = null;
    for (M25Profile__c probe : Trigger.new) {
        probe.LegacyObserved__c = value instanceof String;
    }
}
