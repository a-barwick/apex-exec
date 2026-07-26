public class M28CN10FlowDefinitionActiveOracle {
    public static void run() {
        List<FlowDefinitionView> definitions = [
            SELECT IsActive
            FROM FlowDefinitionView
            WHERE IsActive = TRUE
            LIMIT 1
        ];
        Boolean selectedActive = definitions.isEmpty()
            ? null
            : definitions[0].IsActive;

        System.debug(
            'APEX_EXEC_ORACLE_VALUE|fieldName|' +
            Schema.FlowDefinitionView.IsActive.getDescribe().getName()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|fieldType|' +
            Schema.FlowDefinitionView.IsActive.getDescribe().getType()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|queryExecuted|' +
            (selectedActive == null || selectedActive)
        );
    }
}
