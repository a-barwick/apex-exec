public class NullAwareProfile {
    public static Integer receiverEvaluations = 0;
    public static Integer argumentEvaluations = 0;

    public String name;
    public NullAwareProfile manager;

    public NullAwareProfile(String name) {
        this.name = name;
    }

    public String decorate(Integer suffix) {
        return name + '-' + suffix;
    }

    public static NullAwareProfile select(Boolean present) {
        receiverEvaluations++;
        return present ? new NullAwareProfile('selected') : null;
    }

    public static Integer nextSuffix() {
        argumentEvaluations++;
        return argumentEvaluations;
    }

    public static String normalizedName(NullAwareProfile profile, String fallback) {
        return profile?.name?.trim().toUpperCase() ?? fallback;
    }

    public static String managerName(NullAwareProfile profile) {
        return profile?.manager?.name ?? 'NO MANAGER';
    }

    public static String evaluatedOnce(Boolean present) {
        return select(present)?.decorate(nextSuffix()) ?? 'SKIPPED';
    }

    public static String firstInvoiceName() {
        return [
            SELECT Id, Name
            FROM Invoice__c
            ORDER BY Name
            LIMIT 1
        ]?.Name ?? 'NO INVOICE';
    }

    public static Invoice__c strictFirstInvoice() {
        return [SELECT Id, Name FROM Invoice__c LIMIT 1];
    }

    public static String run() {
        NullAwareProfile manager = new NullAwareProfile('Ada');
        NullAwareProfile engineer = new NullAwareProfile('Grace');
        engineer.manager = manager;

        String result = normalizedName(engineer, 'UNKNOWN')
            + '|' + managerName(engineer)
            + '|' + evaluatedOnce(false)
            + '|' + evaluatedOnce(true);
        System.debug(result);
        System.debug(receiverEvaluations);
        System.debug(argumentEvaluations);
        return result;
    }
}
