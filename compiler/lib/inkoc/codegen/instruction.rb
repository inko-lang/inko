# frozen_string_literal: true

module Inkoc
  module Codegen
    class Instruction
      include Inspect

      NAME_MAPPING = %i[
        Allocate
        AllocatePermanent
        ArrayAllocate
        ArrayAt
        ArrayLength
        ArrayRemove
        ArraySet
        AttributeExists
        BlockGetReceiver
        ByteArrayAt
        ByteArrayEquals
        ByteArrayFromArray
        ByteArrayLength
        ByteArrayRemove
        ByteArraySet
        Close
        CopyBlocks
        CopyRegister
        Exit
        ExternalFunctionCall
        ExternalFunctionLoad
        FloatAdd
        FloatDiv
        FloatEquals
        FloatGreater
        FloatGreaterOrEqual
        FloatMod
        FloatMul
        FloatSmaller
        FloatSmallerOrEqual
        FloatSub
        GeneratorAllocate
        GeneratorResume
        GeneratorValue
        GeneratorYield
        GetAttribute
        GetAttributeInSelf
        GetBuiltinPrototype
        GetFalse
        GetGlobal
        GetLocal
        GetNil
        GetParentLocal
        GetPrototype
        GetTrue
        Goto
        GotoIfFalse
        GotoIfTrue
        IntegerAdd
        IntegerBitwiseAnd
        IntegerBitwiseOr
        IntegerBitwiseXor
        IntegerDiv
        IntegerEquals
        IntegerGreater
        IntegerGreaterOrEqual
        IntegerMod
        IntegerMul
        IntegerShiftLeft
        IntegerShiftRight
        IntegerSmaller
        IntegerSmallerOrEqual
        IntegerSub
        LocalExists
        ModuleGet
        ModuleLoad
        MoveResult
        ObjectEquals
        Panic
        ProcessAddDeferToCaller
        ProcessCurrent
        ProcessIdentifier
        ProcessReceiveMessage
        ProcessSendMessage
        ProcessSetBlocking
        ProcessSetPinned
        ProcessSpawn
        ProcessSuspendCurrent
        ProcessTerminateCurrent
        Return
        RunBlock
        RunBlockWithReceiver
        SetAttribute
        SetBlock
        SetGlobal
        SetLiteral
        SetLiteralWide
        SetLocal
        SetParentLocal
        StringByte
        StringConcat
        StringEquals
        StringLength
        StringSize
        TailCall
        Throw
      ]
        .each_with_index
        .each_with_object({}) { |(value, index), hash| hash[value] = index }
        .freeze

      attr_reader :index, :arguments, :location

      def self.named(name, arguments, location)
        new(NAME_MAPPING.fetch(name), arguments, location)
      end

      def initialize(index, arguments, location)
        @index = index
        @arguments = arguments
        @location = location
      end

      def line
        @location.line
      end
    end
  end
end
