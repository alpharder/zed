(internal_module
  "namespace" @context
  name: (_) @name) @item @kind.namespace

(enum_declaration
  "enum" @context
  name: (_) @name) @item @kind.enum

(type_alias_declaration
  "type" @context
  name: (_) @name) @item @kind.type

(function_declaration
  "async"? @context
  "function" @context
  name: (_) @name
  parameters: (formal_parameters
    "(" @context
    ")" @context)) @item @kind.function

(generator_function_declaration
  "async"? @context
  "function" @context
  "*" @context
  name: (_) @name
  parameters: (formal_parameters
    "(" @context
    ")" @context)) @item @kind.function

(interface_declaration
  "interface" @context
  name: (_) @name) @item @kind.interface

(export_statement
  (lexical_declaration
    [
      "let"
      "const"
    ] @context
    (variable_declarator
      name: (identifier) @name) @item @kind.variable))

; Exported array destructuring
(export_statement
  (lexical_declaration
    [
      "let"
      "const"
    ] @context
    (variable_declarator
      name: (array_pattern
        [
          (identifier) @name @item @kind.variable
          (assignment_pattern
            left: (identifier) @name @item @kind.variable)
          (rest_pattern
            (identifier) @name @item @kind.variable)
        ]))))

; Exported object destructuring
(export_statement
  (lexical_declaration
    [
      "let"
      "const"
    ] @context
    (variable_declarator
      name: (object_pattern
        [
          (shorthand_property_identifier_pattern) @name @item @kind.variable
          (pair_pattern
            value: (identifier) @name @item @kind.variable)
          (pair_pattern
            value: (assignment_pattern
              left: (identifier) @name @item @kind.variable))
          (rest_pattern
            (identifier) @name @item @kind.variable)
        ]))))

(program
  (lexical_declaration
    [
      "let"
      "const"
    ] @context
    (variable_declarator
      name: (identifier) @name) @item @kind.variable))

; Top-level array destructuring
(program
  (lexical_declaration
    [
      "let"
      "const"
    ] @context
    (variable_declarator
      name: (array_pattern
        [
          (identifier) @name @item @kind.variable
          (assignment_pattern
            left: (identifier) @name @item @kind.variable)
          (rest_pattern
            (identifier) @name @item @kind.variable)
        ]))))

; Top-level object destructuring
(program
  (lexical_declaration
    [
      "let"
      "const"
    ] @context
    (variable_declarator
      name: (object_pattern
        [
          (shorthand_property_identifier_pattern) @name @item @kind.variable
          (pair_pattern
            value: (identifier) @name @item @kind.variable)
          (pair_pattern
            value: (assignment_pattern
              left: (identifier) @name @item @kind.variable))
          (rest_pattern
            (identifier) @name @item @kind.variable)
        ]))))

(class_declaration
  "class" @context
  name: (_) @name) @item @kind.class

(abstract_class_declaration
  "abstract" @context
  "class" @context
  name: (_) @name) @item @kind.class

; Method definitions in classes (not in object literals)
(class_body
  (method_definition
    [
      "get"
      "set"
      "async"
      "*"
      "readonly"
      "static"
      (override_modifier)
      (accessibility_modifier)
    ]* @context
    name: (_) @name
    parameters: (formal_parameters
      "(" @context
      ")" @context)) @item @kind.method)

; Object literal methods (including nested objects)
(object
  (method_definition
    [
      "get"
      "set"
      "async"
      "*"
    ]* @context
    name: (_) @name
    parameters: (formal_parameters
      "(" @context
      ")" @context)) @item @kind.method)

(public_field_definition
  [
    "declare"
    "readonly"
    "abstract"
    "static"
    (accessibility_modifier)
  ]* @context
  name: (_) @name) @item @kind.field

; Add support for (node:test, bun:test and Jest) runnable
; Also matches direct modifiers: .skip, .todo, .only, .failing (Jest, Bun, Vitest)
((call_expression
  function: [
    (identifier) @_name
    (member_expression
      object: [
        (identifier) @_name
        (member_expression
          object: (identifier) @_name)
      ])
  ] @context
  (#any-of? @_name "it" "test" "describe" "context" "suite")
  arguments: (arguments
    .
    [
      (string
        (string_fragment) @name)
      (identifier) @name
    ]))) @item @kind.test

; Parameterized and conditional tests. Docs per runner:
;   Jest:   https://jestjs.io/docs/api#testeachtablename-fn-timeout
;   Vitest: https://vitest.dev/api/
;   Bun:    https://bun.sh/docs/test/writing-tests#test-modifiers
((call_expression
  function: (call_expression
    function: (member_expression
      object: [
        (identifier) @_name
        (member_expression
          object: (identifier) @_name)
      ]
      property: (property_identifier) @_property)
    (#any-of? @_name "it" "test" "describe" "context" "suite")
    (#any-of? @_property
      ; Jest, Bun, Vitest
      "each"
      ; Vitest
      "skipIf" "runIf"
      ; Bun
      "if" "todoIf"))
  arguments: (arguments
    .
    [
      (string
        (string_fragment) @name)
      (identifier) @name
    ]))) @item @kind.test

; Object properties
(pair
  key: [
    (property_identifier) @name
    (string
      (string_fragment) @name)
    (number) @name
    (computed_property_name) @name
  ]) @item @kind.property

; Nested variables in function bodies
(statement_block
  (lexical_declaration
    [
      "let"
      "const"
    ] @context
    (variable_declarator
      name: (identifier) @name) @item @kind.variable))

; Nested array destructuring in functions
(statement_block
  (lexical_declaration
    [
      "let"
      "const"
    ] @context
    (variable_declarator
      name: (array_pattern
        [
          (identifier) @name @item @kind.variable
          (assignment_pattern
            left: (identifier) @name @item @kind.variable)
          (rest_pattern
            (identifier) @name @item @kind.variable)
        ]))))

; Nested object destructuring in functions
(statement_block
  (lexical_declaration
    [
      "let"
      "const"
    ] @context
    (variable_declarator
      name: (object_pattern
        [
          (shorthand_property_identifier_pattern) @name @item @kind.variable
          (pair_pattern
            value: (identifier) @name @item @kind.variable)
          (pair_pattern
            value: (assignment_pattern
              left: (identifier) @name @item @kind.variable))
          (rest_pattern
            (identifier) @name @item @kind.variable)
        ]))))

(comment) @annotation
