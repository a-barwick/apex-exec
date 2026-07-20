public with sharing class M27WithOracle {
    public static Integer directCount() {
        return [SELECT COUNT() FROM M27SecureRow__c];
    }

    public static Integer inheritedCount() {
        return M27InheritedOracle.countRows();
    }
}
