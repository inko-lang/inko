# frozen_string_literal: true

module Inkoc
  module Type
    class Block
      include Inspect
      include Predicates
      include ObjectOperations
      include TypeCompatibility
      include GenericTypeOperations

      attr_reader :name, :arguments, :type_parameters, :prototype, :attributes
      attr_accessor :rest_argument, :throws, :returns, :required_arguments_count

      def initialize(name, prototype = nil, returns: nil)
        @name = name
        @prototype = prototype
        @arguments = SymbolTable.new
        @rest_argument = false
        @type_parameters = {}
        @throws = nil
        @returns = returns
        @attributes = SymbolTable.new
        @required_arguments_count = 0
      end

      def arguments_count
        @arguments.length
      end

      def required_arguments_count_without_self
        @required_arguments_count - 1
      end

      def arguments_count_without_self
        arguments_count - 1
      end

      def argument_count_range
        required_arguments_count_without_self..arguments_count_without_self
      end

      def define_self_argument(type)
        define_required_argument(Config::SELF_LOCAL, type)
      end

      def define_required_argument(name, type)
        @required_arguments_count += 1

        arguments.define(name, type)
      end

      def define_argument(name, type)
        arguments.define(name, type)
      end

      def define_rest_argument(name, type)
        @rest_argument = true

        define_argument(name, type)
      end

      def block?
        true
      end

      def return_type
        returns
      end

      def define_type_parameter(name, param)
        @type_parameters[name] = param
      end

      def lookup_argument(name)
        @arguments[name]
      end

      def initialized_return_type(self_type, passed_types)
        param_instances = {}

        arguments.each_with_index do |arg, index|
          next unless arg.type.generated_trait?

          if (concrete_type = passed_types[index])
            param_instances[arg.type.name] = concrete_type
          end
        end

        rtype =
          if return_type.generated_trait?
            param_instances[return_type.name]
          else
            return_type
          end

        rtype.resolve_type(self_type).new_instance(param_instances)
      end

      def lookup_type(name)
        symbol = lookup_attribute(name)

        return symbol.type if symbol.any?

        type_parameters[name]
      end

      def implementation_of?(block)
        arguments_compatible?(block) &&
          type_parameters_compatible?(block) &&
          rest_argument == block.rest_argument &&
          throws == block.throws &&
          returns == block.returns
      end

      def arguments_compatible?(block)
        other_args = block.argument_types_without_self
        args = argument_types_without_self

        return false if args.length != other_args.length

        args.each_with_index do |arg, index|
          return false unless arg.strict_type_compatible?(other_args[index])
        end

        true
      end

      def type_parameter_values
        type_parameters.values
      end

      def type_parameters_compatible?(block)
        params = type_parameter_values
        other_params = block.type_parameter_values

        return false if params.length != other_params.length

        params.each_with_index do |param, index|
          return false unless param.strict_type_compatible?(other_params[index])
        end

        true
      end

      def argument_types_without_self
        types = []

        arguments.each do |arg|
          types << arg.type unless arg.name == Config::SELF_LOCAL
        end

        types
      end

      def type_name
        type_params = type_parameter_names
        args = argument_types_without_self

        tname =
          if type_params.any?
            "#{name}!(#{type_params.join(', ')})"
          else
            name
          end

        tname += "(#{args.map(&:type_name).join(', ')})" unless args.empty?
        tname += " -> #{return_type.type_name}" if return_type

        tname
      end
    end
  end
end
