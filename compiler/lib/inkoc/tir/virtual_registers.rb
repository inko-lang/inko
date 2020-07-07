# frozen_string_literal: true

module Inkoc
  module TIR
    class VirtualRegisters
      include Inspect

      def initialize
        @registers = [VirtualRegister.reserved]
        @used = [true]
      end

      def allocate(type)
        @used.each_with_index do |used, id|
          return new_register(id, type) unless used
        end

        allocate_new(type)
      end

      def allocate_new(type)
        id = @registers.length
        register = new_register(id, type)

        @registers << register
        register
      end

      def allocate_range(types)
        if (range = find_range(types.length))
          range.each_with_index.map do |id, index|
            new_register(id, types.fetch(index))
          end
        else
          types.map { |type| allocate_new(type) }
        end
      end

      def find_range(amount)
        start = nil
        stop = nil

        @used.each_with_index do |used, id|
          if (stop.to_i - start.to_i) == amount
            return start...stop
          elsif used
            start = nil
            stop = nil
          else
            start ||= id
            stop = id
          end
        end

        nil
      end

      def release(register)
        @used[register.id] = false unless register.id.zero?
      end

      def release_all(registers)
        registers.each do |register|
          release(register)
        end
      end

      def new_register(id, type)
        @used[id] = true

        VirtualRegister.new(id, type)
      end

      def length
        @registers.length
      end

      def empty?
        @registers.empty?
      end
    end
  end
end
