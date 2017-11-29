# frozen_string_literal: true

module Inkoc
  module Type
    class Block
      include Inspect
      include Predicates
      include ObjectOperations
      include TypeCompatibility
      include GenericTypeOperations

      attr_reader :name, :arguments, :attributes, :block_type

      attr_accessor :rest_argument, :throws, :returns,
                    :required_arguments_count, :inferred, :prototype,
                    :type_parameters

      def initialize(
        name: Config::BLOCK_TYPE_NAME,
        prototype: nil,
        returns: nil,
        throws: nil,
        type_parameters: TypeParameterTable.new,
        block_type: :closure
      )
        @name = name
        @prototype = prototype
        @arguments = SymbolTable.new
        @rest_argument = false
        @type_parameters = type_parameters
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

      def new_shallow_instance(tparams = type_parameters)
        new_params = TypeParameterTable.new(type_parameters)
        new_params.merge(tparams)

        dup.tap do |copy|
          copy.type_parameters = new_params
        end
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

        range.cover?(given) || given > range.max && rest_argument
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

      def lookup_argument(name)
        arguments[name]
      end

      def type_for_argument(name_or_index)
        arguments[name_or_index].type
      end

      def last_argument_type
        arguments.last.type
      end

      def type_for_argument_or_rest(name_or_index, is_rest = false)
        if is_rest
          last_argument_type
        else
          type_for_argument(name_or_index)
        end
      end

      def implementation_of?(block)
        name == block.name && strict_type_compatible?(block)
      end

      def argument_types_compatible?(other)
        return false if arguments.length != other.arguments.length

        arguments.zip(other.arguments).all? do |ours, theirs|
          ours = real_type_for(ours.type)
          theirs = real_type_for(theirs.type, other.type_parameters)

          ours.type_compatible?(theirs)
        end
      end

      def throw_types_compatible?(other)
        if throws
          if other.throws
            theirs = real_type_for(other.throws, other.type_parameters)

            real_type_for(throws).type_compatible?(theirs)
          else
            closure?
          end
        else
          true
        end
      end

      def return_types_compatible?(other)
        real_type_for(returns)
          .type_compatible?(real_type_for(other.returns, other.type_parameters))
      end

      def type_compatible?(other)
        basic_compat = basic_type_compatibility?(other)

        if basic_compat.nil?
          block_type_compatible?(other)
        else
          basic_compat
        end
      end

      def same_kind_of_block?(other)
        other.block? && block_type == other.block_type
      end

      def block_type_compatible?(other)
        same_kind_of_block?(other) &&
          rest_argument == other.rest_argument &&
          argument_types_compatible?(other) &&
          throw_types_compatible?(other) &&
          return_types_compatible?(other)
      end

      def real_type_for(type, type_params = type_parameters)
        if type&.type_parameter?
          resolved = type_params.instance_for(type.name) || type
          resolved = Type::Optional.new(resolved) if type.optional?

          resolved
        else
          type
        end
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

        args = argument_types_without_self.map do |arg|
          real_type_for(arg).type_name
        end

        tname =
          if type_params.any?
            "#{name} !(#{type_params.join(', ')})"
          else
            name
          end

        tname += " (#{args.join(', ')})" unless args.empty?
        tname += " !! #{real_type_for(throws).type_name}" if throws
        tname += " -> #{real_type_for(return_type).type_name}" if return_type

        tname
      end
    end
  end
end
