# frozen_string_literal: true

module Inkoc
  class MessageContext
    attr_reader :receiver, :block, :arguments, :type_parameters, :location,
                :type_scope

    def initialize(receiver, block, arguments, type_scope, location)
      @receiver = receiver
      @block = block
      @arguments = arguments
      @type_scope = type_scope
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

    def initialize_type_parameter(name, type)
      if receiver.lookup_type_parameter(name)
        receiver.initialize_type_parameter(name, type)
      end

      type_parameters.initialize_parameter(name, type)
    end

    def valid_number_of_arguments?(amount)
      block.valid_number_of_arguments?(amount)
    end

    def initialized_return_type
      rtype = block.return_type
      rtype = rtype.resolve_type(receiver, type_parameters)

      if rtype.initialize_generic_type?
        rtype.new_shallow_instance(type_parameters)
      else
        rtype
      end
    end
  end
end
