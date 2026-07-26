public class M28CN11DynamicQuerySingleCastOracle {
    public static void run() {
        List<Account> created = new List<Account>{
            new Account(Name = 'M28 CN11 one'),
            new Account(Name = 'M28 CN11 two')
        };
        insert created;

        try {
            Id singleId = created[0].Id;
            Account selected = (Account) Database.query(
                'SELECT Id, Name FROM Account WHERE Id = :singleId'
            );
            System.debug(
                'APEX_EXEC_ORACLE_VALUE|singleRecord|' +
                (selected.Name == 'M28 CN11 one')
            );
            Account assigned = Database.query(
                'SELECT Id, Name FROM Account WHERE Id = :singleId'
            );
            System.debug(
                'APEX_EXEC_ORACLE_VALUE|directAssignment|' +
                (assigned.Name == 'M28 CN11 one')
            );

            Boolean emptyException = false;
            try {
                Account missing = (Account) Database.query(
                    'SELECT Id FROM Account WHERE Id = NULL'
                );
                System.debug(missing);
            } catch (QueryException error) {
                emptyException = true;
            }
            System.debug(
                'APEX_EXEC_ORACLE_VALUE|emptyException|' + emptyException
            );

            List<Id> createdIds = new List<Id>{ created[0].Id, created[1].Id };
            Boolean multipleException = false;
            try {
                Account duplicate = (Account) Database.query(
                    'SELECT Id FROM Account WHERE Id IN :createdIds'
                );
                System.debug(duplicate);
            } catch (QueryException error) {
                multipleException = true;
            }
            System.debug(
                'APEX_EXEC_ORACLE_VALUE|multipleException|' +
                multipleException
            );
        } finally {
            delete created;
        }
    }
}
