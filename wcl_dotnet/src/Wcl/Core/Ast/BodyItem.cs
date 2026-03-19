namespace Wcl.Core.Ast
{
    public abstract class BodyItem { }

    public sealed class AttributeItem : BodyItem
    {
        public Attribute Attribute { get; }
        public AttributeItem(Attribute attribute) => Attribute = attribute;
    }

    public sealed class BlockItem : BodyItem
    {
        public Block Block { get; }
        public BlockItem(Block block) => Block = block;
    }

    public sealed class TableItem : BodyItem
    {
        public Table Table { get; }
        public TableItem(Table table) => Table = table;
    }

    public sealed class LetBindingItem : BodyItem
    {
        public LetBinding LetBinding { get; }
        public LetBindingItem(LetBinding letBinding) => LetBinding = letBinding;
    }

    public sealed class MacroDefItem : BodyItem
    {
        public MacroDef MacroDef { get; }
        public MacroDefItem(MacroDef macroDef) => MacroDef = macroDef;
    }

    public sealed class MacroCallItem : BodyItem
    {
        public MacroCall MacroCall { get; }
        public MacroCallItem(MacroCall macroCall) => MacroCall = macroCall;
    }

    public sealed class ForLoopItem : BodyItem
    {
        public ForLoop ForLoop { get; }
        public ForLoopItem(ForLoop forLoop) => ForLoop = forLoop;
    }

    public sealed class ConditionalItem : BodyItem
    {
        public Conditional Conditional { get; }
        public ConditionalItem(Conditional conditional) => Conditional = conditional;
    }

    public sealed class ValidationItem : BodyItem
    {
        public Validation Validation { get; }
        public ValidationItem(Validation validation) => Validation = validation;
    }

    public sealed class SchemaItem : BodyItem
    {
        public Schema Schema { get; }
        public SchemaItem(Schema schema) => Schema = schema;
    }

    public sealed class DecoratorSchemaBodyItem : BodyItem
    {
        public DecoratorSchema DecoratorSchema { get; }
        public DecoratorSchemaBodyItem(DecoratorSchema decoratorSchema) => DecoratorSchema = decoratorSchema;
    }
}
