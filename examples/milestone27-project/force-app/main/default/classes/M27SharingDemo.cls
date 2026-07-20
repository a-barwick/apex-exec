public class M27SharingDemo {
    public static String run() {
        M27Row__c owned = new M27Row__c();
        owned.Name = 'Owned';
        M27Row__c other = new M27Row__c();
        other.Name = 'Other';
        other.OwnerId = '005000000000002AAA';
        insert new List<M27Row__c>{owned, other};

        return String.valueOf(M27WithReader.directCount())
            + '|' + String.valueOf(M27WithReader.inheritedCount())
            + '|' + String.valueOf(M27WithoutReader.directCount())
            + '|' + String.valueOf(M27WithoutReader.inheritedCount())
            + '|' + String.valueOf(M27InheritedReader.countRows());
    }
}
