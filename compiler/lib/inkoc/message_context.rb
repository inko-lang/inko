# frozen_string_literal: true

module Inkoc
  class MessageContext
    attr_reader :receiver, :block, :arguments, :type_parameters, :location,
                :type_scope, :typedb

    def initialize(receiver, block, arguments, type_scope, typedb, location)
      @receiver = receiver
      @block = block
      @arguments = arguments
      @type_scope = type_scope
      @typedb = typedb
      @location = location

      @type_parameters =
        if receiver.type_parameter?
          Type::TypeParameterTable.new
        else
          Type::TypeParameterTable.new(receiver.type_parameters)
        end

      @type_parameters.merge(block.type_parameters)
    end

    def valid_argument_name?(name)
      block.lookup_argument(name).any?
    end

    def argument_types
      arguments.map(&:type)
    end

    def arguments_count_without_self
      block.arguments_count_without_self
    end

    def argument_count_range
      block.argument_count_range
    end

    def rest_argument
      block.rest_argument
    end

    def type_for_argument_or_rest(*args)
      block.type_for_argument_or_rest(*args)
    end

    def type_parameter_instance(name)
      type_parameters.instance_for(name)
    end

    def resolved_instance_for_type_parameter(name)
      instance = type_parameter_instance(name)

      fully_resolve_type(instance) if instance
    end

    def initialize_type_parameter(name, type)
      if !receiver.type_parameter? && receiver.lookup_type_parameter(name)
        receiver.initialize_type_parameter(name, type)
      end

      type_parameters.initialize_parameter(name, type)
    end

    def valid_number_of_arguments?(amount)
      block.valid_number_of_arguments?(amount)
    end

    def fully_resolve_type(type)
      resolved = with_method_requirements(type)
        .resolve_type(receiver, type_parameters)

      if resolved.initialize_generic_type?
        resolved = resolved.new_shallow_instance(type_parameters)
      end

      resolved
    end

    def initialized_return_type
      rtype = fully_resolve_type(block.return_type)

      wrap_optional_return_type(rtype)
    end

    def with_method_requirements(type)
      if type.type_parameter? && (method_type = type_scope.method_block_type)
        type.with_method_requirements(method_type)
      else
        type
      end
    end

    def wrap_optional_return_type(type)
      return type unless receiver.optional?

      # If Nil doesn't define the method we need to wrap the return type in an
      # optional type.
      if typedb.nil_type.lookup_method(block.name).any?
        type
      else
        Type::Optional.wrap(type)
      end
    end
  end
end
