# SDF Schema Reference

This reference documents **177** schemas.

## Hierarchy

```
ai_agent
  actions
    action
      config_ref
    actions_ref
    conditional
      branch
        action (...)
        actions (...)
        actions_ref (...)
  agent
    behaviour
      config_ref (...)
      test_exclusion
      test_hint
    constraint
      config_ref (...)
      test_exclusion (...)
      test_hint (...)
    tool
      behaviour (...)
      constraint (...)
  behaviour (...)
  constraint (...)
  data_store
    behaviour (...)
    constraint (...)
  output
    behaviour (...)
    constraint (...)
  parameter
    behaviour (...)
    constraint (...)
  tool (...)
  trigger
ai_hook
  behaviour (...)
  constraint (...)
  handler
    behaviour (...)
    constraint (...)
ai_marketplace
  behaviour (...)
  constraint (...)
  owner
  plugin_entry
    author
    plugin_source
ai_mcp_server
  behaviour (...)
  constraint (...)
  oauth
ai_output_style
  behaviour (...)
  constraint (...)
ai_plugin
  ai_agent (...)
  ai_hook (...)
  ai_mcp_server (...)
  ai_output_style (...)
  ai_shared_folder
    ai_shared_file
    behaviour (...)
    constraint (...)
  ai_skill
    actions (...)
    agent (...)
    behaviour (...)
    constraint (...)
    data_store (...)
    output (...)
    parameter (...)
    procedure
      behaviour (...)
      conditional (...)
      constraint (...)
      step
        config_ref (...)
    tool (...)
    trigger (...)
  author (...)
  behaviour (...)
  constraint (...)
ai_shared_folder (...)
ai_skill (...)
constitution
  rule
external_system
  behaviour (...)
  constraint (...)
  external_spec
for_block
library
local
macro_block
macro_definition
module
  actions (...)
  behaviour (...)
  component
    api
      api_auth
        behaviour (...)
        constraint (...)
      api_field
        api_field (...)
        behaviour (...)
        constraint (...)
      api_middleware
        behaviour (...)
        constraint (...)
      api_version
      behaviour (...)
      constraint (...)
      gql_mutation
        behaviour (...)
        constraint (...)
        gql_arg
      gql_query
        behaviour (...)
        constraint (...)
        gql_arg (...)
      gql_subscription
        behaviour (...)
        constraint (...)
        gql_arg (...)
      gql_type
        behaviour (...)
        constraint (...)
        gql_field
          behaviour (...)
          constraint (...)
          gql_arg (...)
        union_member
      odata_entity
        api_field (...)
        behaviour (...)
        constraint (...)
        navigation_property
      operation
        behaviour (...)
        constraint (...)
        header_param
        path_param
        query_param
        request_body
          api_field (...)
          behaviour (...)
          constraint (...)
        response
          api_field (...)
          behaviour (...)
          constraint (...)
      rate_limit
      resource
        api_field (...)
        behaviour (...)
        constraint (...)
        operation (...)
        resource (...)
      rpc_namespace
        behaviour (...)
        constraint (...)
        rpc_call
          api_field (...)
          behaviour (...)
          constraint (...)
      rpc_service
        behaviour (...)
        constraint (...)
        rpc_method
          behaviour (...)
          constraint (...)
      soap_service
        behaviour (...)
        constraint (...)
        soap_operation
          behaviour (...)
          constraint (...)
    behaviour (...)
    cli
      behaviour (...)
      constraint (...)
      sub_command
        actions (...)
        behaviour (...)
        constraint (...)
        switch
          behaviour (...)
          constraint (...)
      switch (...)
    config_ref (...)
    configuration
      config_item
      config_scope
    constraint (...)
    database
      behaviour (...)
      collection
        behaviour (...)
        constraint (...)
        document_field
          document_field (...)
        sdf_index
          behaviour (...)
          constraint (...)
      constraint (...)
      edge_type
        behaviour (...)
        constraint (...)
        property
      node_label
        behaviour (...)
        constraint (...)
        property (...)
      reference_data
        behaviour (...)
        constraint (...)
        row
          column_value
      routine
        behaviour (...)
        constraint (...)
        routine_param
      sdf_table
        behaviour (...)
        column
          behaviour (...)
          constraint (...)
        constraint (...)
        foreign_key
          behaviour (...)
          constraint (...)
        sdf_index (...)
      vector_collection
        behaviour (...)
        constraint (...)
        vector_field
    external_spec (...)
    file_format
    implementation
      behaviour (...)
      constraint (...)
      implementation_hint
      state_machine
        state
        transition
    interface
      behaviour (...)
      constraint (...)
      method
        behaviour (...)
        constraint (...)
        method_param
    protocol
      behaviour (...)
      constraint (...)
      endpoint
        behaviour (...)
        constraint (...)
      error_code
        behaviour (...)
        constraint (...)
      header
        behaviour (...)
        constraint (...)
        message_field
          behaviour (...)
          constraint (...)
          message_field (...)
      message
        behaviour (...)
        constraint (...)
        message_field (...)
      protocol_flow
        behaviour (...)
        constraint (...)
        flow_step
          behaviour (...)
          constraint (...)
      sdf_enum
        behaviour (...)
        constraint (...)
        enum_value
          behaviour (...)
          constraint (...)
    sdf_type
      behaviour (...)
      constraint (...)
      field
    security
      authn
      authz
      behaviour (...)
      constraint (...)
      data_classification
      external_spec (...)
      security_constraint
      threat_model
    test
    test_exclusion (...)
    test_hint (...)
    ui
      behaviour (...)
      constraint (...)
      design_system
        design_element
        design_token
      input_mapping
        input_action
      navigation
        flow
      screen
        animation
        behaviour (...)
        breakpoint
        constraint (...)
        layer
        layout
        style
        ui_asset
        ui_component
          accessibility
          animation (...)
          behaviour (...)
          constraint (...)
          data_binding
          layout (...)
          style (...)
          ui_component (...)
        ui_state
      ui_asset (...)
      ui_state (...)
  constraint (...)
project_overview
  goal
  non_goal
repository
  branching_strategy
  commit_convention
    example
    rule (...)
  documentation
    doc_file
      doc_asset
      doc_section
    doc_folder
      doc_file (...)
      doc_folder (...)
  ignore
    ignore_category
    ignore_path
  infrastructure
    behaviour (...)
    constraint (...)
    pipeline
      behaviour (...)
      constraint (...)
      job
        behaviour (...)
        constraint (...)
    task_runner
      behaviour (...)
      constraint (...)
      recipe
        behaviour (...)
        constraint (...)
  issue_labels
    issue_label
  pr_workflow
  remote
  submodule
  system_ref
spec_update_checks
  check
    behaviour (...)
    constraint (...)
system
  actions (...)
  api (...)
  behaviour (...)
  change
    actions (...)
    affected_item
  component (...)
  configuration (...)
  constraint (...)
  database (...)
  external_spec (...)
  file_format (...)
  interface (...)
  protocol (...)
  sdf_type (...)
  security (...)
  security_policy
  test (...)
  ui (...)
user
var
```

## All Schemas

| Schema | Description |
|--------|-------------|
| [_root](schemas/_root.md) | Root document. Only top-level SDF entities are allowed here. |
| [accessibility](schemas/accessibility.md) | Accessibility intent for a UI component. |
| [action](schemas/action.md) | A discrete step or operation within an action sequence. |
| [actions](schemas/actions.md) | A container holding an ordered sequence of steps or actions. |
| [actions_ref](schemas/actions_ref.md) | A reference to a named Actions block defined elsewhere. |
| [affected_item](schemas/affected_item.md) | A spec item impacted by a bug. |
| [agent](schemas/agent.md) | A subagent that a skill or agent can dispatch. |
| [ai_agent](schemas/ai_agent.md) | An AI agent — a delegatable sub-agent. |
| [ai_hook](schemas/ai_hook.md) | A Claude Code hook — an event-driven lifecycle handler. |
| [ai_marketplace](schemas/ai_marketplace.md) | A Claude Code marketplace — a Git repository cataloging plugins. |
| [ai_mcp_server](schemas/ai_mcp_server.md) | A Model Context Protocol (MCP) server. |
| [ai_output_style](schemas/ai_output_style.md) | A Claude Code output style — system prompt modification. |
| [ai_plugin](schemas/ai_plugin.md) | A Claude Code plugin directory containing components that extend Claude Code. |
| [ai_shared_file](schemas/ai_shared_file.md) | A file within a shared AI folder. |
| [ai_shared_folder](schemas/ai_shared_folder.md) | A shared resource folder accessible to AI skills and agents. |
| [ai_skill](schemas/ai_skill.md) | An AI skill — a composable, reusable capability powered by AI. |
| [animation](schemas/animation.md) | A visual animation or micro-interaction. |
| [api](schemas/api.md) | An API endpoint group. |
| [api_auth](schemas/api_auth.md) | An authentication scheme for an API. |
| [api_field](schemas/api_field.md) | A field in an API request or response body. |
| [api_middleware](schemas/api_middleware.md) | Middleware applied to an API. |
| [api_version](schemas/api_version.md) | API versioning strategy. |
| [authn](schemas/authn.md) | An authentication requirement or mechanism. |
| [author](schemas/author.md) | The author of a plugin. |
| [authz](schemas/authz.md) | An authorisation rule or role-based access control. |
| [behaviour](schemas/behaviour.md) | An observable behaviour or capability. |
| [branch](schemas/branch.md) | A branch within a conditional block. |
| [branching_strategy](schemas/branching_strategy.md) | Git branching strategy configuration. |
| [breakpoint](schemas/breakpoint.md) | How a layout adapts at a specific viewport size. |
| [change](schemas/change.md) | A change request — either a feature or a bug fix. |
| [check](schemas/check.md) | A validation rule enforced at one or more lifecycle phases. |
| [cli](schemas/cli.md) | A command-line interface within a component. |
| [collection](schemas/collection.md) | A document database collection. |
| [column](schemas/column.md) | A column in a database table. |
| [column_value](schemas/column_value.md) | A cell value within a seed data row. |
| [commit_convention](schemas/commit_convention.md) | Commit message convention. |
| [component](schemas/component.md) | A logical component within a system or module. |
| [conditional](schemas/conditional.md) | A conditional block with branches. |
| [config_item](schemas/config_item.md) | An individual configuration setting. |
| [config_ref](schemas/config_ref.md) | A reference to a configuration item that influences behaviour. |
| [config_scope](schemas/config_scope.md) | A configuration source with a priority for precedence. |
| [configuration](schemas/configuration.md) | Describes the configuration landscape for a system or component. |
| [constitution](schemas/constitution.md) | Inviolable rules and principles for the project. |
| [constraint](schemas/constraint.md) | A business rule or invariant that must hold. |
| [data_binding](schemas/data_binding.md) | What data drives a UI component. |
| [data_classification](schemas/data_classification.md) | A sensitivity label for data handled by a system or component. |
| [data_store](schemas/data_store.md) | The persistence layer for a skill or agent's state. |
| [database](schemas/database.md) | A database engine backing the system. |
| [design_element](schemas/design_element.md) | A reusable visual element or pattern in the design system. |
| [design_system](schemas/design_system.md) | The visual design language for a UI. |
| [design_token](schemas/design_token.md) | A named design token in the design system. |
| [doc_asset](schemas/doc_asset.md) | A binary asset referenced by a documentation file. |
| [doc_file](schemas/doc_file.md) | A documentation file. |
| [doc_folder](schemas/doc_folder.md) | A directory within a documentation set. |
| [doc_section](schemas/doc_section.md) | A named section within a documentation file. |
| [document_field](schemas/document_field.md) | A field within a document collection. |
| [documentation](schemas/documentation.md) | A documentation set within a repository. |
| [edge_type](schemas/edge_type.md) | A relationship or edge type in a graph database. |
| [endpoint](schemas/endpoint.md) | A named endpoint or operation in a protocol. |
| [enum_value](schemas/enum_value.md) | A named value within a protocol enumeration. |
| [error_code](schemas/error_code.md) | A protocol-level error code. |
| [example](schemas/example.md) | An example within a convention. |
| [external_spec](schemas/external_spec.md) | A reference to an external specification, standard, or RFC. |
| [external_system](schemas/external_system.md) | An external system or third-party dependency. |
| [field](schemas/field.md) | A field within a domain type. |
| [file_format](schemas/file_format.md) | A file format block describing the schema of a file. |
| [flow](schemas/flow.md) | A navigation transition between screens. |
| [flow_step](schemas/flow_step.md) | A single step in a protocol flow. |
| [for_block](schemas/for_block.md) | A dynamic block that generates child blocks at parse time. |
| [foreign_key](schemas/foreign_key.md) | A foreign key constraint referencing another table. |
| [goal](schemas/goal.md) | A project goal to achieve. |
| [gql_arg](schemas/gql_arg.md) | An argument on a GraphQL field or operation. |
| [gql_field](schemas/gql_field.md) | A field on a GraphQL type. |
| [gql_mutation](schemas/gql_mutation.md) | A root GraphQL mutation operation. |
| [gql_query](schemas/gql_query.md) | A root GraphQL query operation. |
| [gql_subscription](schemas/gql_subscription.md) | A root GraphQL subscription operation. |
| [gql_type](schemas/gql_type.md) | A GraphQL type definition. |
| [handler](schemas/handler.md) | A handler that executes when a hook fires. |
| [header](schemas/header.md) | A protocol-level header or framing structure. |
| [header_param](schemas/header_param.md) | An HTTP header parameter. |
| [ignore](schemas/ignore.md) | Declares files and directories to exclude from version control. |
| [ignore_category](schemas/ignore_category.md) | A high-level category of files to ignore. |
| [ignore_path](schemas/ignore_path.md) | An explicit file path or glob pattern to exclude. |
| [implementation](schemas/implementation.md) | Technical implementation details including algorithms, data structures, and state machines. |
| [implementation_hint](schemas/implementation_hint.md) | Guidance for an LLM about implementation approaches. |
| [infrastructure](schemas/infrastructure.md) | Groups infrastructure-as-code concerns for a repository. |
| [input_action](schemas/input_action.md) | A logical input action. |
| [input_mapping](schemas/input_mapping.md) | Maps physical inputs to logical actions. |
| [interface](schemas/interface.md) | A contract between parts of the system. |
| [issue_label](schemas/issue_label.md) | A single issue label mapping. |
| [issue_labels](schemas/issue_labels.md) | Maps GitHub issue labels to SDF change types. |
| [job](schemas/job.md) | A job within a CI/CD pipeline. |
| [layer](schemas/layer.md) | A rendering layer with explicit z-ordering. |
| [layout](schemas/layout.md) | How child components are arranged within a container. |
| [library](schemas/library.md) | A top-level block that declares an installed library. |
| [local](schemas/local.md) | A scoped local variable referenced via ${local.<name>}. |
| [macro_block](schemas/macro_block.md) | A macro invocation that expands a template. |
| [macro_definition](schemas/macro_definition.md) | A reusable block template with named parameters. |
| [message](schemas/message.md) | A named message, packet, or frame in a protocol. |
| [message_field](schemas/message_field.md) | A field within a protocol message. |
| [method](schemas/method.md) | A method signature within an interface. |
| [method_param](schemas/method_param.md) | A parameter on an interface method. |
| [module](schemas/module.md) | A shared library or reusable module. |
| [navigation](schemas/navigation.md) | Navigation patterns between screens. |
| [navigation_property](schemas/navigation_property.md) | An OData navigation property. |
| [node_label](schemas/node_label.md) | A node type or label in a graph database. |
| [non_goal](schemas/non_goal.md) | An explicit non-goal the project will not pursue. |
| [oauth](schemas/oauth.md) | OAuth configuration for an HTTP-based MCP server. |
| [odata_entity](schemas/odata_entity.md) | An OData entity set. |
| [operation](schemas/operation.md) | An HTTP operation on a resource. |
| [output](schemas/output.md) | A typed output produced by a skill or agent. |
| [owner](schemas/owner.md) | The marketplace maintainer. |
| [parameter](schemas/parameter.md) | A typed input parameter for a skill or agent. |
| [path_param](schemas/path_param.md) | A URL path parameter. |
| [pipeline](schemas/pipeline.md) | A CI/CD pipeline workflow. |
| [plugin_entry](schemas/plugin_entry.md) | A plugin entry in a marketplace. |
| [plugin_source](schemas/plugin_source.md) | A structured source for fetching a plugin from an external location. |
| [pr_workflow](schemas/pr_workflow.md) | Pull request conventions for a repository. |
| [procedure](schemas/procedure.md) | A reusable, named sequence of ordered steps within an AI skill. |
| [project_overview](schemas/project_overview.md) | High-level project overview with goals and non-goals. |
| [property](schemas/property.md) | A property on a graph node or edge. |
| [protocol](schemas/protocol.md) | A network protocol definition. |
| [protocol_flow](schemas/protocol_flow.md) | A sequence of message exchanges in a protocol flow. |
| [query_param](schemas/query_param.md) | A URL query parameter. |
| [rate_limit](schemas/rate_limit.md) | Rate limiting rules for an API. |
| [recipe](schemas/recipe.md) | A public recipe or task exposed by a task runner. |
| [reference_data](schemas/reference_data.md) | A named set of reference/seed data for a table. |
| [remote](schemas/remote.md) | A remote hosting location for a repository. |
| [repository](schemas/repository.md) | A source code repository. |
| [request_body](schemas/request_body.md) | The request body for an API operation. |
| [resource](schemas/resource.md) | A REST resource group (e.g. /users, /sessions). |
| [response](schemas/response.md) | A response for an API operation. |
| [routine](schemas/routine.md) | A server-side programmable object (stored procedure, function, or trigger). |
| [routine_param](schemas/routine_param.md) | A parameter on a database routine. |
| [row](schemas/row.md) | A row of seed or reference data. |
| [rpc_call](schemas/rpc_call.md) | A JSON-RPC method. |
| [rpc_method](schemas/rpc_method.md) | A gRPC method within a service. |
| [rpc_namespace](schemas/rpc_namespace.md) | A JSON-RPC namespace grouping. |
| [rpc_service](schemas/rpc_service.md) | A gRPC service definition. |
| [rule](schemas/rule.md) | An inviolable rule or principle. |
| [screen](schemas/screen.md) | A screen or page within a user interface. |
| [sdf_enum](schemas/sdf_enum.md) | A named enumeration in a protocol. |
| [sdf_index](schemas/sdf_index.md) | A database index for query performance. |
| [sdf_table](schemas/sdf_table.md) | A relational database table. |
| [sdf_type](schemas/sdf_type.md) | A domain type definition. |
| [security](schemas/security.md) | A container for security-related requirements and controls. |
| [security_constraint](schemas/security_constraint.md) | A specific security rule that must be enforced. |
| [security_policy](schemas/security_policy.md) | An organisation-wide security policy. |
| [soap_operation](schemas/soap_operation.md) | A SOAP operation. |
| [soap_service](schemas/soap_service.md) | A SOAP web service. |
| [spec_update_checks](schemas/spec_update_checks.md) | Validation rules enforced at various spec lifecycle phases. |
| [state](schemas/state.md) | A state within a state machine. |
| [state_machine](schemas/state_machine.md) | A formal finite state machine model. |
| [step](schemas/step.md) | A single step within a procedure. |
| [style](schemas/style.md) | A named set of visual properties. |
| [sub_command](schemas/sub_command.md) | A sub-command within a CLI tool. |
| [submodule](schemas/submodule.md) | A git submodule reference. |
| [switch](schemas/switch.md) | A CLI flag or option. |
| [system](schemas/system.md) | A deployable system or application. |
| [system_ref](schemas/system_ref.md) | A reference to a system or module's location within a repository. |
| [task_runner](schemas/task_runner.md) | A task runner file and its public recipes. |
| [test](schemas/test.md) | Testing strategy and framework configuration for a system or component. |
| [test_exclusion](schemas/test_exclusion.md) | Marks something as explicitly excluded from testing. |
| [test_hint](schemas/test_hint.md) | A hint for a scenario that should be tested. |
| [threat_model](schemas/threat_model.md) | A known threat and its mitigations. |
| [tool](schemas/tool.md) | A tool the skill or agent is allowed to use. |
| [transition](schemas/transition.md) | A transition between states in a state machine. |
| [trigger](schemas/trigger.md) | An activation trigger for a skill or agent. |
| [ui](schemas/ui.md) | A user interface surface. |
| [ui_asset](schemas/ui_asset.md) | A media asset used by the interface. |
| [ui_component](schemas/ui_component.md) | A visual component within a screen or another component. |
| [ui_state](schemas/ui_state.md) | Local UI state. |
| [union_member](schemas/union_member.md) | A member type of a GraphQL union. |
| [user](schemas/user.md) | A human or AI actor that interacts with the system. |
| [var](schemas/var.md) | A global variable referenced via ${var.<name>}. |
| [vector_collection](schemas/vector_collection.md) | A vector collection or index for similarity search. |
| [vector_field](schemas/vector_field.md) | A metadata field stored alongside vectors. |
