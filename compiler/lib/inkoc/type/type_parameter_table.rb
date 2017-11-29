# frozen_string_literal: true

module Inkoc
  module Type
    class TypeParameterTable
      include Enumerable

      def initialize(copy_from = nil)
        @parameters = []
        @map = {}
        @instances = {}

        merge(copy_from) if copy_from
      end

      def merge(table)
        table.each do |param|
          @parameters << param
          @map[param.name] = param
        end

        table.each_instance do |name, type|
          initialize_parameter(name, type)
        end
      end

      def initialize_in_order(source)
        if source.is_a?(self.class)
          initialize_in_order_from_table(source)
        else
          initialize_in_order_from_array(source)
        end
      end

      def initialize_in_order_from_table(table)
        table.each_instance.each_with_index do |(_, instance), index|
          next unless (our_param = self[index])

          initialize_parameter(our_param.name, instance)
        end
      end

      def initialize_in_order_from_array(array)
        array.each_with_index do |instance, index|
          next unless (our_param = self[index])

          initialize_parameter(our_param.name, instance)
        end
      end

      def names
        @parameters.map(&:name)
      end

      def each(&block)
        return to_enum(__method__) unless block_given?

        @parameters.each(&block)
      end

      def each_instance(&block)
        @instances.each(&block)
      end

      def any?
        @parameters.any?
      end

      def length
        @parameters.length
      end
      alias size length
      alias count length

      def empty?
        @parameters.empty?
      end

      def define(name, required_traits = [])
        param = TypeParameter.new(name: name, required_traits: required_traits)

        @map[param.name] = param
        @parameters << param

        param
      end

      def initialize_parameter(name, type)
        @instances[name] = type if @map.key?(name)
      end

      def instance_for(name)
        @instances[name]
      end

      def [](name_or_index)
        if name_or_index.is_a?(Numeric)
          @parameters[name_or_index]
        else
          @map[name_or_index]
        end
      end
    end
  end
end
