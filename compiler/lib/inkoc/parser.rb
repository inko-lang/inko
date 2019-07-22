# frozen_string_literal: true

module Inkoc
  # rubocop: disable Metrics/ClassLength
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
        mut
        for
        impl
        try
        do
        lambda
      ]
    ).freeze

    VALUE_START = Set.new(
      %i[
        attribute
        bracket_open
        constant
        curly_open
        define
        do
        float
        hash_open
        identifier
        impl
        integer
        lambda
        let
        let
        paren_open
        return
        self
        string
        throw
        trait
        try
        try_bang
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

    # rubocop: disable Metrics/CyclomaticComplexity
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
      when :module_documentation
        module_documentation(start)
      else
        expression(start)
      end
    end
    # rubocop: enable Metrics/CyclomaticComplexity

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
          symbol = import_symbol_from_token(step)
          symbols << AST::ImportSymbol.new(symbol, nil, step.location)
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
      type_cast(start)
    end

    def type_cast(start)
      node = binary_send(start)

      while @lexer.next_type_is?(:as)
        advance!

        type = type(advance!)
        node = AST::TypeCast.new(node, type, start.location)
      end

      node
    end

    def binary_send(start)
      node = send_chain(start)

      while BINARY_OPERATORS.include?(@lexer.peek.type)
        operator = @lexer.advance
        rhs = send_chain(@lexer.advance, require_same_line: true)
        node = AST::Send.new(operator.value, node, [], [rhs], operator.location)
      end

      # This allows us to parse code such as this:
      #
      #     x == y
      #       .if_true {
      #
      #       }
      #
      # Into this:
      #
      #     (x == y).if_true {
      #
      #     }
      if @lexer.next_type_is?(:dot)
        skip_one
        node = send_chain_with_receiver(node)
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
          "Expected a closing bracket, got #{@lexer.peek.type.inspect} instead"
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

    # Parses a type name.
    #
    # Examples:
    #
    #     Foo
    #     Foo!(Bar)
    #     Foo::Bar!(Baz)
    def type_name(token)
      AST::TypeName
        .new(constant(token), optional_type_parameters, token.location)
    end

    # Parses a block type.
    #
    # Examples:
    #
    #     do
    #     do (A)
    #     do (A, B)
    #     do (A) -> R
    #     do (A) !! X -> R
    def block_type(start, type = :do)
      args = block_type_arguments
      throws = optional_throw_type
      returns = optional_return_type
      klass = type == :lambda ? AST::LambdaType : AST::BlockType

      klass.new(args, returns, throws, start.location)
    end

    def block_type_arguments
      args = []

      if @lexer.next_type_is?(:paren_open)
        skip_one

        while (token = @lexer.advance) && token.valid_but_not?(:paren_close)
          args << type_name(token)

          break if comma_or_break_on(:paren_close)
        end
      end

      args
    end

    # Parses a type argument.
    def def_type_parameter(token)
      node = AST::DefineTypeParameter.new(token.value, token.location)

      if @lexer.next_type_is?(:colon)
        skip_one

        node.required_traits = required_traits
      end

      node
    end

    # Parses a chain of messages being sent to a receiver.
    def send_chain(start, require_same_line: false)
      node = value(start)

      loop do
        case @lexer.peek.type
        when :dot
          break if require_same_line && @lexer.peek.line != start.line

          skip_one
          node = send_chain_with_receiver(node)
        when :bracket_open
          # Only treat [x][y] as a send if [y] occurs on the same line. This
          # ensures that e.g. [x]\n[y] is parsed as two array literals.
          break unless @lexer.peek.line == start.line

          bracket = @lexer.advance
          name, args = bracket_get_or_set

          node = AST::Send.new(name, node, [], args, bracket.location)
        when :exclamation
          node = AST::Dereference.new(node, advance!.location)
        else
          break
        end
      end

      node
    end

    def send_chain_with_receiver(receiver)
      name_start = advance!
      name = message_name_for_token(name_start)
      location = name_start.location
      args = []
      peeked = @lexer.peek

      if peeked.type == :type_args_open
        skip_one

        type_args = type_parameters
        peeked = @lexer.peek
      else
        type_args = []
      end

      if peeked.type == :paren_open && peeked.line == location.line
        args = arguments_with_parenthesis
      elsif next_expression_is_argument?(name_start)
        return send_without_parenthesis(receiver, name, type_args, location)
      end

      AST::Send.new(name, receiver, type_args, args, location)
    end

    # Returns true if the next expression is an argument to use when parsing
    # arguments without parenthesis.
    def next_expression_is_argument?(current)
      peeked = @lexer.peek
      current_end = current.value.length + current.column

      # Something is only an argument if:
      #
      # 1. It resides on the same line.
      # 2. It is separated by at least a single space.
      VALUE_START.include?(peeked.type) &&
        peeked.line == current.line &&
        (peeked.column - current_end) >= 1
    end

    # Parses a list of send arguments wrapped in parenthesis.
    #
    # Example:
    #
    #     (10, 'foo', 'bar')
    #     (10, 'foo', 'bar') do { ... }
    # rubocop: disable Metrics/CyclomaticComplexity
    # rubocop: disable Metrics/PerceivedComplexity
    def arguments_with_parenthesis
      args = []
      paren_line = nil

      # Skip the opening parenthesis
      skip_one

      while (token = @lexer.advance) && token.valid?
        if token.type == :paren_close
          paren_line = token.line
          break
        end

        args << expression_or_keyword_argument(token)

        if @lexer.next_type_is?(:comma)
          skip_one
        elsif @lexer.peek.valid_but_not?(:paren_close)
          raise ParseError, "Expected a comma, not #{@lexer.peek.value.inspect}"
        end
      end

      # If a block follows the send on the same line as the closing parenthesis,
      # we include it as the last argument.
      if (trailing_block = trailing_block_for_send(paren_line))
        args << trailing_block
      end

      args
    end
    # rubocop: enable Metrics/PerceivedComplexity
    # rubocop: enable Metrics/CyclomaticComplexity

    def trailing_block_for_send(paren_line)
      return unless @lexer.peek.line == paren_line

      case @lexer.peek.type
      when :curly_open
        block_without_arguments(advance!)
      when :do, :lambda
        token = advance!

        block(token, token.type)
      end
    end

    # Parses a list of send arguments without parenthesis.
    #
    # Example:
    #
    #     foo 10, 'foo', 'bar'
    def send_without_parenthesis(receiver, name, type_arguments, location)
      args = []

      while (token = @lexer.advance) && token.valid?
        arg, is_block = argument_for_send_without_parenthesis(token)
        args << arg

        if is_block && @lexer.next_type_is?(:dot)
          skip_one

          node = AST::Send.new(name, receiver, type_arguments, args, location)

          return send_chain_with_receiver(node)
        end

        break unless @lexer.next_type_is?(:comma)

        skip_one
      end

      AST::Send.new(name, receiver, type_arguments, args, location)
    end

    def argument_for_send_without_parenthesis(token)
      case token.type
      when :curly_open
        [block_without_arguments(token), true]
      when :do
        [block(token), true]
      when :lambda
        [block(token, :lambda), true]
      else
        [expression_or_keyword_argument(token), false]
      end
    end

    def expression_or_keyword_argument(start)
      if @lexer.next_type_is?(:colon)
        skip_one

        value = expression(advance!)

        AST::KeywordArgument.new(start.value, value, start.location)
      else
        expression(start)
      end
    end

    # rubocop: disable Metrics/AbcSize
    # rubocop: disable Metrics/CyclomaticComplexity
    def value(start)
      case start.type
      when :string then string(start)
      when :integer then integer(start)
      when :float then float(start)
      when :identifier then identifier_or_reassign(start)
      when :constant then constant(start)
      when :curly_open then block_without_arguments(start)
      when :bracket_open then array(start)
      when :hash_open then hash(start)
      when :define then def_method(start)
      when :static then def_static_method(start)
      when :do, :lambda then block(start, start.type)
      when :let then let_define(start)
      when :return then return_value(start)
      when :attribute then attribute_or_reassign(start)
      when :self then self_object(start)
      when :throw then throw_value(start)
      when :try then try(start)
      when :try_bang then try_bang(start)
      when :colon_colon then global(start)
      when :paren_open then grouped_expression
      when :documentation then documentation(start)
      else
        raise ParseError, "A value can not start with a #{start.type.inspect}"
      end
    end
    # rubocop: enable Metrics/AbcSize
    # rubocop: enable Metrics/CyclomaticComplexity

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
      peeked = @lexer.peek

      if peeked.type == :type_args_open
        skip_one

        type_args = type_parameters
        peeked = @lexer.peek
      else
        type_args = []
      end

      if peeked.type == :paren_open && peeked.line == start.line
        args = arguments_with_parenthesis

        AST::Send.new(start.value, nil, type_args, args, start.location)
      elsif next_expression_is_argument?(start)
        # If an identifier is followed by another expression on the same line
        # we'll treat said expression as the start of an argument list.
        send_without_parenthesis(nil, start.value, type_args, start.location)
      else
        identifier_from_token(start, type_args)
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
      token = advance!

      name =
        if token.type == :identifier || token.type == :constant
          token.value
        else
          raise(
            ParseError,
            "Unexpected #{token.type}, expected an identifier or constant"
          )
        end

      AST::Global.new(name, start.location)
    end

    # Parses a grouped expression.
    #
    # Example:
    #
    #   (10 + 20)
    def grouped_expression
      expr = expression(advance!)

      advance_and_expect!(:paren_close)

      expr
    end

    # Parses a block without arguments.
    #
    # Examples:
    #
    #     { body }
    def block_without_arguments(start)
      loc = start.location

      AST::Block.new([], [], nil, nil, block_body(start), loc, signature: false)
    end

    # Parses a block starting with the "do" keyword.
    #
    # Examples:
    #
    #     do { body }
    #     do (arg) { body }
    #     do (arg: T) { body }
    #     do (arg: T) -> T { body }
    def block(start, type = :do)
      targs = optional_type_parameter_definitions
      args = optional_arguments
      throw_type = optional_throw_type
      ret_type = optional_return_type
      body = block_body(advance_and_expect!(:curly_open))
      klass = type == :lambda ? AST::Lambda : AST::Block

      klass.new(targs, args, ret_type, throw_type, body, start.location)
    end

    # Parses the body of a block.
    def block_body(start)
      nodes = []

      while (token = @lexer.advance) && token.valid_but_not?(:curly_close)
        nodes << expression(token)
      end

      AST::Body.new(nodes, start.location)
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
    #     %['key': 'value']
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
      receiver = AST::Global.new(Config::HASH_MAP_LITERAL_RECEIVER, location)
      keys_array = new_array(keys, start)
      vals_array = new_array(vals, start)

      AST::Send.new(
        Config::FROM_ARRAY_MESSAGE,
        receiver,
        [],
        [keys_array, vals_array],
        location
      )
    end

    # Parses a method definition.
    #
    # Examples:
    #
    #     def foo { ... }
    #     def foo
    #     def foo -> A { ... }
    #     def foo!(T)(arg: T) -> T { ... }
    #     def foo !! B -> A { ... }
    #     def foo !! B -> A where A: B { ... }
    def def_method(start)
      name_token = advance!
      name = message_name_for_token(name_token)
      targs = optional_type_parameter_definitions
      arguments = optional_arguments
      throw_type = optional_throw_type
      ret_type = optional_return_type
      required = false
      requirements = optional_method_requirements

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
        requirements,
        body,
        start.location
      )
    end

    def def_static_method(start)
      def_start = advance_and_expect!(:define)

      def_method(def_start).tap do |method|
        method.static = true
      end
    end

    # Parses a list of argument definitions.
    # rubocop: disable Metrics/CyclomaticComplexity
    def def_arguments
      args = []

      while (token = advance!) && token.valid_but_not?(:paren_close)
        token, rest = advance_if_rest_argument(token)
        token, mutable = advance_if_mutable_argument(token)

        if token.type != :identifier
          raise(ParseError, "Expected an identifier, not #{token.type}")
        end

        type = optional_argument_type
        default = optional_argument_default unless rest

        args << AST::DefineArgument
          .new(token.value, type, default, rest, mutable, token.location)

        break if comma_or_break_on(:paren_close) || rest
      end

      args
    end
    # rubocop: enable Metrics/CyclomaticComplexity

    def advance_if_rest_argument(token)
      if token.type == :mul
        [advance!, true]
      else
        [token, false]
      end
    end

    def advance_if_mutable_argument(token)
      if token.type == :mut
        [advance!, true]
      else
        [token, false]
      end
    end

    def optional_argument_type
      return unless @lexer.next_type_is?(:colon)

      skip_one

      type(advance!)
    end

    def optional_argument_default
      return unless @lexer.next_type_is?(:assign)

      skip_one

      expression(advance!)
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
        args << type(advance!)

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

      type(advance!)
    end

    def optional_throw_type
      return unless @lexer.next_type_is?(:throws)

      skip_one

      type(advance!)
    end

    def optional_method_requirements
      return [] unless @lexer.next_type_is?(:where)

      skip_one

      method_requirements
    end

    def method_requirements
      requirements = []

      while @lexer.next_type_is?(:constant)
        param = advance_and_expect!(:constant)

        advance_and_expect!(:colon)

        requirements << AST::MethodRequirement
          .new(param.value, required_traits, param.location)

        break unless @lexer.next_type_is?(:comma)

        skip_one
      end

      requirements
    end

    def required_traits
      required = []

      loop do
        required << type_name(advance_and_expect!(:constant))

        break unless @lexer.next_type_is?(:add)

        skip_one
      end

      required
    end

    # Parses a definition of a variable.
    #
    # Example:
    #
    #     let number = 10
    #     let mut number = 10
    def let_define(start)
      mutable =
        if @lexer.next_type_is?(:mut)
          skip_one
          true
        else
          false
        end

      name = variable_name
      vtype = optional_variable_type
      value = variable_value

      AST::DefineVariable.new(name, value, vtype, mutable, start.location)
    end

    def def_attribute(start)
      advance_and_expect!(:colon)

      vtype = type(advance!)

      AST::DefineAttribute.new(start.value, vtype, start.location)
    end

    # Parses the name of a variable definition.
    def variable_name
      start = advance!

      case start.type
      when :identifier then identifier_from_token(start)
      when :constant then constant_from_token(start)
      else
        raise(
          ParseError,
          "Unexpected #{start.type}, expected an identifier or constant "
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
      type(advance!)
    end

    def variable_value
      advance_and_expect!(:assign)

      expression(advance!)
    end

    # Parses an object definition.
    #
    # Examples:
    #
    #     object Person {}
    #     object Person impl Foo {}
    #     object Person impl Foo, Bar {}
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
        node =
          case token.type
          when :define then def_method(token)
          when :static then def_static_method(token)
          when :attribute then def_attribute(token)
          when :documentation then documentation(token)
          else
            raise ParseError, "A #{token.type.inspect} is not valid here"
          end

        nodes << node
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

      body = trait_body(advance_and_expect!(:curly_open))

      AST::Trait.new(name.value, targs, required_traits, body, start.location)
    end

    def trait_body(start)
      nodes = []

      while (token = @lexer.advance) && token.valid_but_not?(:curly_close)
        nodes << expression(token)
      end

      AST::Body.new(nodes, start.location)
    end

    # Parses a list of traits that must be implemented by whatever implements
    # the current trait.
    def trait_requirements
      required = []

      while @lexer.next_type_is?(:constant)
        required << constant(advance!)

        advance! if @lexer.next_type_is?(:add)
      end

      required
    end

    # Parses the implementation of a trait or re-opening of an object.
    #
    # Example:
    #
    #     impl ToString for Object {
    #       ...
    #     }
    def implement_trait(start)
      trait_or_object_name = type_name(advance_and_expect!(:constant))

      if @lexer.next_type_is?(:for)
        advance_and_expect!(:for)

        object_name = type_name(advance_and_expect!(:constant))
        body = block_body(advance_and_expect!(:curly_open))

        AST::TraitImplementation.new(
          trait_or_object_name,
          object_name,
          body,
          start.location
        )
      else
        body = block_body(advance_and_expect!(:curly_open))

        AST::ReopenObject
          .new(trait_or_object_name, body, start.location)
      end
    end

    # Parses a return statement.
    #
    # Example:
    #
    #     return 10
    def return_value(start)
      value = expression(advance!) if next_expression_is_argument?(start)

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
      value = AST::Send.new(message, variable, [], [rhs], location)

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
      expression = try_expression
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

    # Parses a "try!" statement
    def try_bang(start)
      expression = try_expression
      else_arg, else_body = try_bang_else(start)

      AST::Try.new(expression, else_body, else_arg, start.location)
    end

    def try_expression
      with_curly =
        if @lexer.next_type_is?(:curly_open)
          advance!
          true
        end

      expression = expression(advance!)

      advance_and_expect!(:curly_close) if with_curly

      expression
    end

    def try_bang_else(start)
      arg = try_bang_else_arg(start)
      loc = start.location

      body = [
        # _INKOC.panic(error.to_string)
        AST::Send.new(
          Config::PANIC_MESSAGE,
          AST::Constant.new(Config::RAW_INSTRUCTION_RECEIVER, nil, loc),
          [],
          [AST::Send.new(Config::TO_STRING_MESSAGE, arg, [], [], loc)],
          loc
        )
      ]

      [arg, AST::Body.new(body, loc)]
    end

    def try_bang_else_arg(start)
      AST::Identifier.new('error', start.location)
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

    # Parses a type
    #
    # Examples:
    #
    #     Integer
    #     ?Integer
    #     (T) -> R
    def type(start)
      optional =
        if start.type == :question
          start = advance!
          true
        else
          false
        end

      type =
        case start.type
        when :constant
          type_name(start)
        when :do, :lambda
          block_type(start, start.type)
        else
          raise(
            ParseError,
            "Unexpected #{start.type}, expected a constant or a ("
          )
        end

      type.optional = optional

      type
    end

    def documentation(start)
      documentation_comment_of_type(start, :documentation, AST::Documentation)
    end

    def module_documentation(start)
      documentation_comment_of_type(
        start,
        :module_documentation,
        AST::ModuleDocumentation
      )
    end

    def documentation_comment_of_type(start, type, klass)
      values = [start.value] + token_sequence_values(type)

      klass.new(values.join("\n"), start.location)
    end

    def token_sequence_values(type)
      values = []

      values << advance!.value while @lexer.next_type_is?(type)

      values
    end

    def constant_from_token(token, receiver = nil)
      AST::Constant.new(token.value, receiver, token.location)
    end

    def identifier_from_token(token, type_arguments = [])
      if type_arguments.any?
        AST::Send.new(token.value, nil, type_arguments, [], token.location)
      else
        AST::Identifier.new(token.value, token.location)
      end
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

      AST::Send.new('new', receiver, [], values, start.location)
    end
  end
  # rubocop: enable Metrics/ClassLength
end
