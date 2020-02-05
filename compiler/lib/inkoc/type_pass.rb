# frozen_string_literal: true

module Inkoc
  module TypePass
    def initialize(mod, state)
      @module = mod
      @state = state
      @constant_resolver = ConstantResolver.new(diagnostics)
    end

    def diagnostics
      @state.diagnostics
    end

    def typedb
      @state.typedb
    end

    def run(ast)
      locals = ast.locals
      scope = TypeScope
        .new(@module.type, @module.body.type, @module, locals: locals)

      on_module_body(ast, scope)

      [ast]
    end

    def on_module_body(node, scope)
      define_type(node, scope)
      nil
    end

    def on_body(node, scope)
      define_types(node.expressions, scope)
      nil
    end

    def on_constant(node, scope)
      @constant_resolver.resolve(node, scope)
    end

    def on_dynamic_type(node, _)
      wrap_optional_type(node, TypeSystem::Dynamic.new)
    end

    def on_never_type(node, _)
      wrap_optional_type(node, TypeSystem::Never.new)
    end

    def on_self_type_with_late_binding(node, _)
      wrap_optional_type(node, TypeSystem::SelfType.new)
    end

    def on_self_type(node, scope)
      self_type = scope.self_type

      # When "Self" translates to a generic type, e.g. Array!(T), we want to
      # return a type in the form of `Array!(T -> T)`, and not just `Array`.
      # This ensures that any arguments passed to a method returning "Self" can
      # properly initialise the type.
      type_arguments =
        self_type.generic_type? ? self_type.type_parameters.to_a : []

      wrap_optional_type(node, self_type.new_instance(type_arguments))
    end

    def on_type_name(node, scope)
      type = define_type(node.name, scope)

      return type if type.error?
      return wrap_optional_type(node, type) unless type.generic_type?

      # When our type is a generic type we need to initialise it according to
      # the passed type parameters.
      type_arguments = node
        .type_parameters
        .zip(type.type_parameters)
        .map do |param_node, param|
          param_instance = define_type_instance(param_node, scope)

          if param && !param_instance.type_compatible?(param, @state)
            return diagnostics
                .type_error(param, param_instance, param_node.location)
          end

          param_instance
        end

      num_given = type_arguments.length
      num_expected = type.type_parameters.length

      if num_given != num_expected
        return diagnostics.type_parameter_count_error(
          num_given,
          num_expected,
          node.location
        )
      end

      # Simply referencing a constant should not lead to it being initialised,
      # unless there are any type parameters to initialise.
      wrap_optional_type(
        node,
        type.new_instance_for_reference(type_arguments)
      )
    end

    def define_type(node, scope, *extra)
      type = process_node(node, scope, *extra)

      node.type ||= type if type
    end

    def define_types(nodes, scope, *extra)
      nodes.map { |n| define_type(n, scope, *extra) }
    end

    def define_type_instance(node, scope, *extra)
      type = define_type(node, scope, *extra)

      if type && !type.type_instance?
        type = type.new_instance
        node.type = type
      end

      type
    end

    def wrap_optional_type(node, type)
      if node.optional?
        TypeSystem::Optional.wrap(type)
      else
        type
      end
    end

    def store_type(type, scope, location)
      scope.self_type.define_attribute(type.name, type)

      store_type_as_global(type.name, type, scope, location)

      type
    end

    def store_type_as_global(name, type, scope, location)
      if Config::RESERVED_CONSTANTS.include?(name)
        diagnostics.redefine_reserved_constant_error(name, location)
      elsif scope.module_scope?
        @module.globals.define(name, type)
      end
    end

    def scope_for_object_body(node)
      TypeScope
        .new(node.type, node.block_type, @module, locals: node.body.locals)
    end

    def define_required_traits(node, trait, scope)
      node.required_traits.each do |req_node|
        req = define_type_instance(req_node, scope)

        trait.add_required_trait(req) unless req.error?
      end
    end
  end
end
