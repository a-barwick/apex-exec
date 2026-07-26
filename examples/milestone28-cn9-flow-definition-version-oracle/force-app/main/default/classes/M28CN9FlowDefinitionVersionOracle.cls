public class M28CN9FlowDefinitionVersionOracle {
    public static void run() {
        List<FlowDefinitionView> definitions = [
            SELECT VersionNumber
            FROM FlowDefinitionView
            LIMIT 1
        ];
        Integer selectedVersion = definitions.isEmpty()
            ? null
            : definitions[0].VersionNumber;

        System.debug(
            'APEX_EXEC_ORACLE_VALUE|fieldName|' +
            Schema.FlowDefinitionView.VersionNumber.getDescribe().getName()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|fieldType|' +
            Schema.FlowDefinitionView.VersionNumber.getDescribe().getType()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|queryExecuted|' +
            (selectedVersion == null || selectedVersion >= 0)
        );
    }
}
