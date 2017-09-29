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

      def initialize(name, prototype = nil)
        @name = name
        @prototype = prototype
        @arguments = SymbolTable.new
        @rest_argument = false
        @type_parameters = {}
        @throws = nil
        @returns = nil
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

      def type_name
        tname = super
        tname += " -> #{return_type.type_name}" if return_type

        tname
      end
    end
  end
end
