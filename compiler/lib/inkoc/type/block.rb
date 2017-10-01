# frozen_string_literal: true

module Inkoc
  module Type
    class Block
      include Inspect
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

      def arguments_count_without_self
        @arguments.length - 1
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

      def initialized_return_type(passed_types)
        instance = return_type.new_instance

        arguments.each_with_index do |arg, index|
          next unless arg.type.type_parameter?

          if (concrete_type = passed_types[index])
            instance.init_type_parameter(arg.type.name, concrete_type)
          end
        end

        instance
      end

      def lookup_type(name)
        symbol = lookup_attribute(name)

        return symbol.type if symbol.any?

        type_parameters[name]
      end

      def implementation_of?(block)
        arguments_compatible?(block) &&
          type_parameters == block.type_parameters &&
          rest_argument == block.rest_argument &&
          throws == block.throws &&
          returns == block.returns
      end

      def arguments_compatible?(block)
        other_types = block.argument_types_without_self

        argument_types_without_self.each_with_index do |arg, index|
          other = other_types[index]

          return false unless arg.strict_type_compatible?(other)
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
        tname = super
        args = argument_types_without_self

        tname += "(#{args.map(&:type_name).join(', ')})" unless args.empty?
        tname += " -> #{return_type.type_name}" if return_type

        tname
      end
    end
  end
end
