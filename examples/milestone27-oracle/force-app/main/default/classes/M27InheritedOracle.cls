public inherited sharing class M27InheritedOracle {
    public static Integer countRows() {
        return [SELECT COUNT() FROM M27SecureRow__c];
    }
}
