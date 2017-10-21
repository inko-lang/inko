# frozen_string_literal: true

module Inkoc
  class Parser
    ParseError = Class.new(StandardError)

    MESSAGE_TOKENS = Set.new(
      %i[
        add
        and
        as
        bitwise_and
        bitwise_or
        bitwise_xor
        bracket_open
        constant
        div
        else
        equal
        exclusive_range
        greater
        greater_equal
        identifier
        impl
        import
        inclusive_range
        let
        lower
        lower_equal
        mod
        mul
        not_equal
        object
        or
        pow
        return
        self
        shift_left
        shift_right
        sub
        throw
        trait
        var
        for
        impl
      ]
    ).freeze

    VALUE_START = Set.new(
      %i[
        attribute
        bracket_open
        constant
        curly_open
        float
        define
        do
        hash_open
        identifier
        impl
        integer
        let
        let
        return
        self
        string
        sub
        throw
        trait
        try
      ]
    ).freeze

    BINARY_OPERATORS = Set.new(
      %i[
        or
        and
        equal
        not_equal
        lower
        lower_equal
        greater
        greater_equal
        bitwise_or
        bitwise_xor
        bitwise_and
        shift_left
        shift_right
        add
        sub
        div
        mod
        mul
        pow
        inclusive_range
        exclusive_range
      ]
    ).freeze

    BINARY_REASSIGN_OPERATORS = Set.new(
      %i[
        div_assign
        mod_assign
        bitwise_xor_assign
        bitwise_and_assign
        bitwise_or_assign
        pow_assign
        mul_assign
        sub_assign
        add_assign
        shift_left_assign
        shift_right_assign
      ]
    ).freeze

    def initialize(input, file_path = Pathname.new('(eval)'))
      @lexer = Lexer.new(input, file_path)
    end

    def location
      @lexer.current_location
    end

    def line
      @lexer.line
    end

    def column
      @lexer.column
    end

    def parse
      expressions
    end

    def expressions
      location = @lexer.current_location
      children = []

      while (token = @lexer.advance) && token.valid?
        children << top_level(token)
      end

      AST::Body.new(children, location)
    end

    def top_level(start)
      case start.type
      when :import
        import(start)
      when :object
        def_object(start)
      when :trait
        def_trait(start)
      when :impl
        implement_trait(start)
      when :compiler_option_open
        compiler_option
      else
        expression(start)
      end
    end

    # Parses an import statement.
    #
    # Examples:
    #
    #     import foo
    #     import foo::bar
    #     import foo::bar::(Baz as Bla)
    def import(start)
      steps = []
      symbols = []
      step = advance_and_expect!(:identifier)

      loop do
        case step.type
        when :identifier, :object, :trait
          steps << identifier_from_token(step)
        when :constant
          symbols << AST::ImportSymbol.new(step.value, nil, step.location)
          break
        when :mul
          symbols << AST::GlobImport.new(step.location)
          break
        else
          raise ParseError, "#{step.type} is not valid in import statements"
        end

        break unless @lexer.next_type_is?(:colon_colon)

        skip_one

        if @lexer.next_type_is?(:paren_open)
          skip_one
          symbols = import_symbols
          break
        end

        step = advance!
      end

      AST::Import.new(steps, symbols, start.location)
    end

    def import_symbols
      symbols = []

      loop do
        start = advance!
        symbol = import_symbol_from_token(start)

        alias_name =
          if @lexer.next_type_is?(:as)
            skip_one
            import_alias_from_token(advance!)
          end

        symbols << AST::ImportSymbol.new(symbol, alias_name, start.location)

        break if comma_or_break_on(:paren_close)
      end

      symbols
    end

    def import_symbol_from_token(start)
      case start.type
      when :identifier, :constant
        identifier_from_token(start)
      when :self
        self_object(start)
      else
        raise(
          ParseError,
          "#{start.type.inspect} is not a valid import symbol"
        )
      end
    end

    def import_alias_from_token(start)
      case start.type
      when :identifier, :constant
        identifier_from_token(start)
      else
        raise(
          ParseError,
          "#{start.type.inspect} is not a valid symbol alias"
        )
      end
    end

    def expression(start)
      binary_send(start)
    end

    def binary_send(start)
      node = bracket_send(start)

      while BINARY_OPERATORS.include?(@lexer.peek.type)
        operator = @lexer.advance
        rhs = bracket_send(@lexer.advance)
        node = AST::Send.new(operator.value, node, [rhs], operator.location)
      end

      node
    end

    def bracket_send(start)
      start_line = start.line
      node = send_chain(start)

      while @lexer.next_type_is?(:bracket_open)
        # Only treat [x][y] as a send if [y] occurs on the same line. This
        # ensures that e.g. [x]\n[y] is parsed as two array literals.
        break unless @lexer.peek.line == start_line

        bracket = @lexer.advance
        name, args = bracket_get_or_set

        node = AST::Send.new(name, node, args, bracket.location)
      end

      node
    end

    # Parses an expression such as `[X]` or `[X] = Y`.
    def bracket_get_or_set
      args = []

      while (token = @lexer.advance) && token.valid_but_not?(:bracket_close)
        args << expression(token)

        if @lexer.next_type_is?(:comma)
          @lexer.advance
          next
        end

        next if @lexer.peek.type == :bracket_close

        raise(
          ParseError,
          "Expected a closing bracket, got #{peeked.type.inspect} instead"
        )
      end

      name = if @lexer.next_type_is?(:assign)
               args << expression(skip_and_advance!)

               '[]='
             else
               '[]'
             end

      [name, args]
    end

    # Parses a type name or a nullable type.
    #
    # Examples:
    #
    #     Foo
    #     Foo!(Bar)
    #     Foo::Bar!(Baz)
    def type_name(token)
      node = constant(token)

      node.type_parameters = optional_type_parameters
      node.return_type = optional_return_type

      node
    end

    # Parses a type argument.
    def def_type_parameter(token)
      node = AST::DefineTypeParameter.new(token.value, token.location)

      if @lexer.next_type_is?(:colon)
        skip_one

        loop do
          node.required_traits << type_name(advance_and_expect!(:constant))

          break unless @lexer.next_type_is?(:add)

          skip_one
        end
      end

      node
    end

    # Parses a chain of messages being sent to a receiver.
    def send_chain(start)
      node = value(start)

      while @lexer.next_type_is?(:dot)
        skip_one

        name, location = send_name_and_location
        args = send_chain_arguments(location.line)
        node = AST::Send.new(name, node, args, location)
      end

      node
    end

    # Returns the name and location to use for sending a message to an object.
    def send_name_and_location
      token = advance!

      [message_name_for_token(token), token.location]
    end

    # Parses the arguments for a method call part of a method call chain.
    def send_chain_arguments(line)
      if @lexer.next_type_is?(:paren_open)
        arguments_with_parenthesis
      elsif next_expression_is_argument?(line)
        arguments_without_parenthesis
      else
        []
      end
    end

    # Returns true if the next expression is an argument to use when parsing
    # arguments without parenthesis.
    def next_expression_is_argument?(line)
      peeked = @lexer.peek

      VALUE_START.include?(peeked.type) && peeked.line == line
    end

    # Parses a list of send arguments wrapped in parenthesis.
    #
    # Example:
    #
    #     (10, 'foo', 'bar')
    def arguments_with_parenthesis
      args = []

      # Skip the opening parenthesis
      skip_one

      while (token = @lexer.advance) && token.valid?
        break if token.type == :paren_close

        args << send_argument(token)

        if @lexer.next_type_is?(:comma)
          skip_one
        elsif @lexer.peek.valid_but_not?(:paren_close)
          raise ParseError, "Expected a comma, not #{@lexer.peek.value.inspect}"
        end
      end

      args
    end

    # Parses a list of send arguments without parenthesis.
    #
    # Example:
    #
    #     10, 'foo', 'bar'
    def arguments_without_parenthesis
      args = []

      while (token = @lexer.advance) && token.valid?
        args << send_argument(token)

        break unless @lexer.next_type_is?(:comma)

        skip_one
      end

      args
    end

    # Parses an argument passed to a method call.
    #
    # Examples:
    #
    #     foo(10)
    #     foo(number: 10)
    def send_argument(start)
      if @lexer.next_type_is?(:colon)
        skip_one

        value = expression(advance!)

        AST::KeywordArgument.new(start.value, value, start.location)
      else
        # Blocks are parsed more "tightly" so that `foo { }.bar` translates into
        # `foo({ }).bar`.
        case start.type
        when :curly_open
          block_without_arguments(start)
        when :do
          block(start)
        else
          expression(start)
        end
      end
    end

    # rubocop: disable Metrics/CyclomaticComplexity
    # rubocop: disable Metrics/AbcSize
    def value(start)
      case start.type
      when :string then string(start)
      when :integer then integer(start)
      when :float then float(start)
      when :identifier then identifier_or_reassign(start)
      when :constant then constant(start)
      when :curly_open then block_without_arguments(start)
      when :sub then negative_number(start)
      when :bracket_open then array(start)
      when :hash_open then hash(start)
      when :define then def_method(start)
      when :do then block(start)
      when :let then let_define(start)
      when :var then var_define(start)
      when :return then return_value(start)
      when :attribute then attribute_or_reassign(start)
      when :self then self_object(start)
      when :throw then throw_value(start)
      when :try then try(start)
      when :colon_colon then global(start)
      else
        raise ParseError, "A value can not start with a #{start.type.inspect}"
      end
    end

    def string(start)
      AST::String.new(start.value, start.location)
    end

    def integer(start)
      AST::Integer.new(Integer(start.value), start.location)
    end

    def float(start)
      AST::Float.new(Float(start.value), start.location)
    end

    def identifier_or_reassign(start)
      return reassign_local(start) if @lexer.next_type_is?(:assign)

      node = identifier(start)

      if next_is_binary_reassignment?
        reassign_binary(node)
      else
        node
      end
    end

    def identifier(start)
      if @lexer.next_type_is?(:paren_open)
        args = arguments_with_parenthesis

        AST::Send.new(start.value, nil, args, start.location)
      elsif next_expression_is_argument?(start.line)
        # If an identifier is followed by another expression on the same line
        # we'll treat said expression as the start of an argument list.
        args = arguments_without_parenthesis

        AST::Send.new(start.value, nil, args, start.location)
      else
        identifier_from_token(start)
      end
    end

    # Parses a constant.
    #
    # Examples:
    #
    #     Foo
    #     Foo::Bar
    def constant(start)
      node = constant_from_token(start)

      while @lexer.next_type_is?(:colon_colon)
        skip_one

        start = advance_and_expect!(:constant)
        node = constant_from_token(start, node)
      end

      node
    end

    # Parses a reference to a module global.
    #
    # Example:
    #
    #     ::Foo
    def global(start)
      name = advance_and_expect!(:constant)

      AST::Global.new(name.value, start.location)
    end

    # Parses a block without arguments.
    #
    # Examples:
    #
    #     { body }
    def block_without_arguments(start)
      AST::Block.new([], [], nil, nil, block_body(start), start.location)
    end

    # Parses a block starting with the "fn" keyword.
    #
    # Examples:
    #
    #     fn { body }
    #     fn(arg) { body }
    #     fn(arg: T) { body }
    #     fn(arg: T) -> T { body }
    def block(start)
      targs = optional_type_parameter_definitions
      args = optional_arguments
      ret_type = optional_return_type
      throw_type = optional_throw_type
      body = block_body(advance_and_expect!(:curly_open))

      AST::Block.new(targs, args, ret_type, throw_type, body, start.location)
    end

    # Parses the body of a block.
    def block_body(start)
      nodes = []

      while (token = @lexer.advance) && token.valid_but_not?(:curly_close)
        nodes << expression(token)
      end

      AST::Body.new(nodes, start.location)
    end

    # Parses a negative number.
    #
    # Examples:
    #
    #     -10
    #     -10.5
    def negative_number(start)
      following = advance!

      case following.type
      when :integer
        AST::Integer.new(-Integer(following.value), start.location)
      when :float
        AST::Float.new(-Float(following.value), start.location)
      else
        raise(
          ParseError,
          "Unexpected #{following.type}, expected a number instead"
        )
      end
    end

    # Parses an array literal
    #
    # Example:
    #
    #     [10, 20, 30]
    def array(start)
      values = []

      while (token = @lexer.advance) && token.valid_but_not?(:bracket_close)
        values << expression(token)

        break if comma_or_break_on(:bracket_close)
      end

      new_array(values, start)
    end

    # Parses a hash map literal
    #
    # Example:
    #
    #     %{ 'key': 'value' }
    def hash(start)
      keys = []
      vals = []

      while (key_tok = @lexer.advance) && key_tok.valid_but_not?(:bracket_close)
        key = expression(key_tok)

        advance_and_expect!(:colon)

        value = expression(advance!)

        keys << key
        vals << value

        break if comma_or_break_on(:bracket_close)
      end

      location = start.location
      receiver = AST::Global.new(Config::HASH_MAP_CONST, location)
      keys_array = new_array(keys, start)
      vals_array = new_array(vals, start)

      AST::Send.new('from_array', receiver, [keys_array, vals_array], location)
    end

    # Parses a method definition.
    #
    # Examples:
    #
    #     fn foo { ... }
    #     fn foo
    #     fn foo -> A { ... }
    #     fn foo!(T)(arg: T) -> T { ... }
    #     fn foo -> A throw B { ... }
    def def_method(start)
      name_token = advance!
      name = message_name_for_token(name_token)
      targs = optional_type_parameter_definitions
      arguments = optional_arguments
      ret_type = optional_return_type
      throw_type = optional_throw_type
      required = false

      body =
        if @lexer.next_type_is?(:curly_open)
          block_body(advance!)
        else
          required = true
          AST::Body.new([], start.location)
        end

      AST::Method.new(
        name,
        arguments,
        targs,
        ret_type,
        throw_type,
        required,
        body,
        start.location
      )
    end

    # Parses a list of argument definitions.
    def def_arguments
      args = []

      while @lexer.peek.valid_but_not?(:paren_close)
        rest = if @lexer.next_type_is?(:mul)
                 @lexer.advance
                 true
               else
                 false
               end

        name = advance_and_expect!(:identifier)
        type = optional_argument_type

        default = if @lexer.next_type_is?(:assign)
                    skip_one
                    expression(advance!)
                  end

        args << AST::DefineArgument
          .new(name.value, type, default, rest, name.location)

        break if comma_or_break_on(:paren_close) || rest
      end

      args
    end

    def optional_argument_type
      return unless @lexer.next_type_is?(:colon)

      skip_one

      type_name_or_optional_type(advance!)
    end

    # Parses a list of type argument definitions.
    def def_type_parameters
      args = []

      while @lexer.peek.valid_but_not?(:paren_close)
        args << def_type_parameter(advance_and_expect!(:constant))

        break if comma_or_break_on(:paren_close)
      end

      args
    end

    def type_parameters
      args = []

      while @lexer.peek.valid_but_not?(:paren_close)
        args << type_name(advance_and_expect!(:constant))

        break if comma_or_break_on(:paren_close)
      end

      args
    end

    def optional_arguments
      if @lexer.next_type_is?(:paren_open)
        skip_one
        def_arguments
      else
        []
      end
    end

    def optional_type_parameter_definitions
      if @lexer.next_type_is?(:type_args_open)
        skip_one
        def_type_parameters
      else
        []
      end
    end

    def optional_type_parameters
      if @lexer.next_type_is?(:type_args_open)
        skip_one
        type_parameters
      else
        []
      end
    end

    def optional_return_type
      return unless @lexer.next_type_is?(:arrow)

      skip_one

      type_name_or_optional_type(advance!)
    end

    def optional_throw_type
      return unless @lexer.next_type_is?(:throws)

      skip_one

      type_name_or_optional_type(advance!)
    end

    # Parses a definition of an immutable variable.
    #
    # Example:
    #
    #     let number = 10
    def let_define(start)
      name = variable_name
      vtype = optional_variable_type
      value = variable_value

      AST::DefineVariable.new(name, value, vtype, false, start.location)
    end

    # Parses a definition of a mutable variable.
    #
    # Example:
    #
    #     var number = 10
    def var_define(start)
      name = variable_name
      vtype = optional_variable_type
      value = variable_value

      AST::DefineVariable.new(name, value, vtype, true, start.location)
    end

    # Parses the name of a variable definition.
    def variable_name
      start = advance!

      case start.type
      when :identifier then identifier_from_token(start)
      when :attribute then attribute_from_token(start)
      when :constant then constant_from_token(start)
      else
        raise(
          ParseError,
          "Unexpected #{start.type}, expected an identifier, " \
            'constant or attribute'
        )
      end
    end

    # Parses the optional definition of a variable type.
    #
    # Example:
    #
    #     let x: Integer = 10
    def optional_variable_type
      return unless @lexer.next_type_is?(:colon)

      skip_one
      type_name_or_optional_type(advance!)
    end

    def variable_value
      advance_and_expect!(:assign)

      expression(advance!)
    end

    # Parses an object definition.
    #
    # Example:
    #
    #     object Person {
    #       ...
    #     }
    def def_object(start)
      name = advance_and_expect!(:constant)
      targs = optional_type_parameter_definitions
      body = object_body(advance_and_expect!(:curly_open))

      AST::Object.new(name.value, targs, body, start.location)
    end

    # Parses the body of an object definition.
    def object_body(start)
      nodes = []

      while (token = @lexer.advance) && token.valid_but_not?(:curly_close)
        nodes <<
          case token.type
          when :object
            def_object(token)
          when :trait
            def_trait(token)
          else
            expression(token)
          end
      end

      AST::Body.new(nodes, start.location)
    end

    # Parses the definition of a trait.
    #
    # Examples:
    #
    #     trait Foo { ... }
    #     trait Foo!(T) { ... }
    #     trait Numeric: Add, Subtract { ... }
    def def_trait(start)
      name = advance_and_expect!(:constant)
      targs = optional_type_parameter_definitions

      required_traits =
        if @lexer.next_type_is?(:colon)
          skip_one
          trait_requirements
        else
          []
        end

      body = object_body(advance_and_expect!(:curly_open))

      AST::Trait.new(name.value, targs, required_traits, body, start.location)
    end

    # Parses a list of traits that must be implemented by whatever implements
    # the current trait.
    def trait_requirements
      required = []

      while @lexer.next_type_is?(:constant)
        required << constant(advance!)

        advance! if @lexer.next_type_is?(:comma)
      end

      required
    end

    # Parses the implementation of a trait.
    #
    # Example:
    #
    #     impl ToString for Object {
    #
    #     }
    def implement_trait(start)
      trait_name = constant(advance_and_expect!(:constant))

      advance_and_expect!(:for)

      object_name = constant(advance_and_expect!(:constant))
      body = block_body(advance_and_expect!(:curly_open))

      AST::TraitImplementation
        .new(trait_name, object_name, body, start.location)
    end

    # Parses a return statement.
    #
    # Example:
    #
    #     return 10
    def return_value(start)
      value = expression(advance!) if next_expression_is_argument?(start.line)

      AST::Return.new(value, start.location)
    end

    def attribute_or_reassign(start)
      return reassign_attribute(start) if @lexer.next_type_is?(:assign)

      node = attribute(start)

      if next_is_binary_reassignment?
        reassign_binary(node)
      else
        node
      end
    end

    # Parses an attribute.
    #
    # Examples:
    #
    #     @foo
    def attribute(start)
      attribute_from_token(start)
    end

    # Parses the re-assignment of a local variable.
    #
    # Example:
    #
    #     foo = 10
    def reassign_local(start)
      name = identifier_from_token(start)

      reassign_variable(name, start.location)
    end

    # Parses the re-assignment of an attribute.
    #
    # Example:
    #
    #     @foo = 10
    def reassign_attribute(start)
      name = attribute_from_token(start)

      reassign_variable(name, start.location)
    end

    # Parses the reassignment of a variable.
    #
    # Examples:
    #
    #     a = 10
    #     @a = 10
    def reassign_variable(name, location)
      advance_and_expect!(:assign)

      value = expression(advance!)

      AST::ReassignVariable.new(name, value, location)
    end

    # Parses a binary reassignment of a variable
    #
    # Examples:
    #
    #   a |= 10
    #   @a <<= 20
    def reassign_binary(variable)
      operator = advance!
      location = operator.location
      message = operator.value[0..-2]
      rhs = expression(advance!)
      value = AST::Send.new(message, variable, [rhs], location)

      AST::ReassignVariable.new(variable, value, location)
    end

    def next_is_binary_reassignment?
      BINARY_REASSIGN_OPERATORS.include?(@lexer.peek.type)
    end

    def self_object(start)
      AST::Self.new(start.location)
    end

    # Parses a "throw" statement.
    #
    # Example:
    #
    #     throw Foo
    def throw_value(start)
      value = expression(advance!)

      AST::Throw.new(value, start.location)
    end

    # Parses a "try" statement.
    #
    # Examples:
    #
    #     try foo
    #     try foo else bar
    #     try foo else (error) { error }
    def try(start)
      expression = expression(advance!)
      else_arg = nil

      else_body =
        if @lexer.next_type_is?(:else)
          skip_one

          else_arg = optional_else_arg

          block_with_optional_curly_braces
        else
          AST::Body.new([], start.location)
        end

      AST::Try.new(expression, else_body, else_arg, start.location)
    end

    def block_with_optional_curly_braces
      if @lexer.next_type_is?(:curly_open)
        block_body(@lexer.advance)
      else
        start = advance!

        AST::Body.new([expression(start)], start.location)
      end
    end

    # Parses an optional argument for the "else" statement.
    def optional_else_arg
      return unless @lexer.next_type_is?(:paren_open)

      skip_one

      name = identifier_from_token(advance_and_expect!(:identifier))

      advance_and_expect!(:paren_close)

      name
    end

    # Parses a single regular or optional type.
    #
    # Examples:
    #
    #     fn foo -> Integer
    #     fn foo -> ?Integer
    def type_name_or_optional_type(start)
      optional =
        if start.type == :question
          start = advance_and_expect!(:constant)
          true
        else
          false
        end

      type = type_name(start)
      type.optional = optional

      type
    end

    # Parses a compiler option
    #
    # Example:
    #
    #     ![key: value]
    def compiler_option
      key = advance_and_expect!(:identifier)

      advance_and_expect!(:colon)

      val = advance_and_expect!(:identifier).value
      opt = AST::CompilerOption.new(key.value, val, key.location)

      advance_and_expect!(:bracket_close)

      opt
    end

    def constant_from_token(token, receiver = nil)
      AST::Constant.new(token.value, receiver, token.location)
    end

    def identifier_from_token(token)
      AST::Identifier.new(token.value, token.location)
    end

    def attribute_from_token(token)
      AST::Attribute.new(token.value, token.location)
    end

    def message_name_for_token(token)
      unless MESSAGE_TOKENS.include?(token.type)
        raise ParseError, "#{token.value.inspect} is not a valid message name"
      end

      name = token.value

      if token.type == :bracket_open
        advance_and_expect!(:bracket_close)

        name << ']'
      end

      if @lexer.next_type_is?(:assign)
        skip_one
        name << '='
      end

      name
    end

    def starting_location
      @starting_location ||= SourceLocation.new(1, 1, @lexer.file)
    end

    def skip_and_advance!
      @lexer.advance
      advance!
    end

    def skip_one
      @lexer.advance
    end

    def skip_one_if(type)
      skip_one if @lexer.peek.type == type
    end

    def advance!
      token = @lexer.advance

      raise(ParseError, 'Unexpected end of input') if token.nil?

      token
    end

    def advance_and_expect!(type)
      token = advance!

      return token if token.type == type

      raise(
        ParseError,
        "Expected a #{type.inspect}, got #{token.type.inspect} instead"
      )
    end

    def comma_or_break_on(break_on)
      token = @lexer.advance

      case token.type
      when :comma
        false
      when break_on
        true
      else
        raise(
          ParseError,
          "Unexpected #{token.type}, expected a comma or #{break_on}"
        )
      end
    end

    def new_array(values, start)
      receiver = AST::Global.new(Config::ARRAY_CONST, start.location)

      AST::Send.new('new', receiver, values, start.location)
    end
  end
end
