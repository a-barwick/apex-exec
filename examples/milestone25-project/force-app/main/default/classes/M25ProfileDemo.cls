public class M25ProfileDemo {
    public static String run() {
        M25Profile__c probe = new M25Profile__c();
        probe.Name = 'local';
        insert probe;
        return M25Legacy.nullIsString()
            + '|' + M25Current.nullIsString()
            + '|' + probe.LegacyObserved__c
            + '|' + M25Legacy.evaluations
            + '|' + M25Current.evaluations
            + '|' + M25Current.currentSyntax();
    }
}
