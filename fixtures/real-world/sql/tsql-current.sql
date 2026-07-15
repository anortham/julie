CREATE TABLE [edr].[EdrForms] ([Id] int IDENTITY(1,1) NOT NULL);
GO

MERGE dbo.Target AS t
USING (VALUES (N'a', N'b')) AS s (ColA, ColB)
ON t.ColA = s.ColA
WHEN NOT MATCHED THEN INSERT (ColA, ColB) VALUES (s.ColA, s.ColB);
