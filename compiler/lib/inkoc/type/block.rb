# frozen_string_literal: true

module Inkoc
  module Type
    class Block
      include Inspect
      include Predicates
      include ObjectOperations
      include TypeCompatibility
      include GenericTypeOperations

      attr_reader :name, :arguments, :type_parameters, :attributes, :block_type

      attr_accessor :rest_argument, :throws, :returns,
                    :required_arguments_count, :inferred, :prototype

      def initialize(
        name: Config::BLOCK_TYPE_NAME,
        prototype: nil,
        returns: nil,
        throws: nil,
        block_type: :closure
      )
        @name = name
        @prototype = prototype
        @arguments = SymbolTable.new
        @rest_argument = false
        @type_parameters = {}
        @throws = throws
        @returns = returns || Type::Dynamic.new
        @attributes = SymbolTable.new
        @required_arguments_count = 0
        @block_type = block_type
        @inferred = false
      end

      def implemented_traits
        prototype ? prototype.implemented_traits : Set.new
      end

      def infer?
        closure? && !@inferred
      end

      # Tries to infer this blocks argument types and return type to the types
      # of the given block.
      #
      # If the block could be inferred this method returns true, otherwise false
      # is returned.
      def infer_to(block)
        args = argument_types_without_self
        other_args = block.argument_types_without_self

        args_inferred = args.zip(other_args).all? do |ours, theirs|
          if ours.unresolved_constraint?
            ours.infer_to(theirs)
          else
            true
          end
        end

        return false unless args_inferred

        valid =
          if returns.unresolved_constraint?
            returns.infer_to(block.returns)
          else
            true
          end

        @inferred = true if valid

        valid
      end

      def closure?
        @block_type == :closure
      end

      def method?
        @block_type == :method
      end

      def valid_number_of_arguments?(given)
        range = argument_count_range
        covers = range.cover?(given)

        covers || given > range.max && rest_argument
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

      def self_argument
        arguments[Config::SELF_LOCAL]
      end

      def define_self_argument(type)
        define_required_argument(Config::SELF_LOCAL, type)
      end

      def define_required_argument(name, type, mutable = false)
        @required_arguments_count += 1

        arguments.define(name, type, mutable)
      end

      def define_argument(name, type, mutable = false)
        arguments.define(name, type, mutable)
      end

      def define_rest_argument(name, type, mutable = false)
        @rest_argument = true

        define_argument(name, type, mutable)
      end

      def block?
        true
      end

      def return_type
        returns
      end

      def define_type_parameter(name, param)
        type_parameters[name] = param
      end

      def lookup_argument(name)
        arguments[name]
      end

      def type_for_argument_or_rest(name_or_index)
        arguments[name_or_index].or_else { arguments.last }.type
      end

      def initialized_return_type(self_type, passed_types = [])
        param_instances = {}

        argument_types_without_self.each_with_index do |arg_type, index|
          next unless arg_type.generated_trait?

          if (concrete_type = passed_types[index])
            param_instances[arg_type.name] = concrete_type
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
        name == block.name && strict_type_compatible?(block)
      end

      def type_parameter_values
        type_parameters.values
      end

      def type_parameters_compatible?(block)
        params = type_parameter_values
        other_params = block.type_parameter_values

        return false if params.length != other_params.length

        params.zip(other_params).all? do |ours, theirs|
          ours.strict_type_compatible?(theirs)
        end
      end

      def argument_types_compatible?(other)
        return false if arguments.length != other.arguments.length

        arguments.zip(other.arguments).all? do |ours, theirs|
          ours.type.type_compatible?(theirs.type)
        end
      end

      def throw_types_compatible?(other)
        if throws
          if other.throws
            throws.type_compatible?(other.throws)
          else
            closure?
          end
        else
          true
        end
      end

      def return_types_compatible?(other)
        returns.type_compatible?(other.returns)
      end

      def type_compatible?(other)
        if basic_type_compatibility?(other)
          true
        else
          block_type_compatible?(other)
        end
      end

      def block_type_compatible?(other)
        other.is_a?(self.class) &&
          block_type == other.block_type &&
          rest_argument == other.rest_argument &&
          argument_types_compatible?(other) &&
          throw_types_compatible?(other) &&
          return_types_compatible?(other)
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
        args = argument_types_without_self.map(&:type_name)

        tname =
          if type_params.any?
            "#{name} !(#{type_params.join(', ')})"
          else
            name
          end

        tname += " (#{args.join(', ')})" unless args.empty?
        tname += " !! #{throws.type_name}" if throws
        tname += " -> #{return_type.type_name}" if return_type

        tname
      end
    end
  end
end
