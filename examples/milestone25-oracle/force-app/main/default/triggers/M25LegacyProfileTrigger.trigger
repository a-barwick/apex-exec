trigger M25LegacyProfileTrigger on M25VersionProbe__c (before insert) {
    Object value = null;
    for (M25VersionProbe__c probe : Trigger.new) {
        probe.LegacyObserved__c = value instanceof String;
    }
}
