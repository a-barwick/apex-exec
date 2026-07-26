public class M28CN7OrgLimitsOracle {
    public static void run() {
        Map<String, System.OrgLimit> qualified = System.OrgLimits.getMap();
        Map<String, System.OrgLimit> unqualified = OrgLimits.getMap();
        System.OrgLimit singleEmail = qualified.get('SingleEmail');

        System.debug(
            'APEX_EXEC_ORACLE_VALUE|containsSingleEmail|' +
            (singleEmail != null)
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|nameMatchesKey|' +
            (singleEmail.getName() == 'SingleEmail')
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|usageNonnegative|' +
            (singleEmail.getValue() >= 0)
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|capacityAvailable|' +
            (singleEmail.getValue() < singleEmail.getLimit())
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|aliasesAgree|' +
            (
                unqualified.get('SingleEmail').getName() ==
                singleEmail.getName()
            )
        );
    }
}
